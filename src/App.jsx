import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import {
  adjustPageZoom,
  isDesktopApp,
  isTauri,
  onMenuCommand,
  toggleFullscreen,
} from './lib/platform';
import WaveformEditor from './components/WaveformEditor';
import MenuBar from './components/MenuBar';
import HelpCorner from './components/HelpCorner';
import DeskHeader from './components/faceplate/DeskHeader.jsx';
import Key from './components/faceplate/Key.jsx';
import Lamp from './components/faceplate/Lamp.jsx';
import RecordCheckKeys from './components/recorder/RecordCheckKeys.jsx';
import DeviceSettingsModal from './components/DeviceSettingsModal';
import LinkPanel from './components/link/LinkPanel.jsx';
import ObsPanel from './components/obs/ObsPanel.jsx';
import OutputRouter from './components/desk/OutputRouter.jsx';
import LiveView from './components/desk/LiveView.jsx';
import DesktopRecordButton from './components/recorder/DesktopRecordButton.jsx';
import InputRack from './components/desk/InputRack.jsx';
import MixerConsole from './components/mixer/MixerConsole';
import { createMixerController, createMockMixerController } from './lib/mixerEngine';
import { syncStreamLeveler } from './lib/streamLeveler';
import { getExportAvailability } from './lib/exportFlow';
import { loadRnnoiseAssets } from './lib/rnnoise';
import { pickVideoMimeType } from './lib/mediaFormats';
import { readVisualiserPalette } from './lib/visualiserPalette.js';
import { computerAudioFromEngineStatus } from './lib/computerAudio.js';
import { MONITOR_PREF_KEY, readMonitorPref, setNativeMonitor } from './lib/nativeMonitor.js';
import {
  analyserRange, bandEdges, bandLevels, createScope, createWaterfall, dbFraction, drawScope, fitCanvas, gateState, stepScope,
} from './lib/liveScope.js';
import {
  describeBroadcastRoute,
  devicesEqual,
  mixerRefreshKey,
  normalizeTauriDevices,
  reconcileSelectedDevices,
  resolveBroadcastLabel,
  resolveDeviceLabel,
} from './lib/audioRouting';
import { newNoiseProfileId, nextNoiseProfileNumber } from './lib/noiseProfiles.js';
import { SNAPSHOTS_KEY, nextSnapshotId, parseSnapshots, serializeSnapshots } from './lib/voiceSnapshots.js';
import {
  captureNativeNoiseProfile,
  describeNativeEngineError,
  getNativeEngineStatus,
  learnNativeMicFingerprint,
  startNativeEngine,
  stopNativeEngine,
  subscribeToNativeMeters,
  syncNativeParams,
} from './lib/nativeEngine';
import { trackEvent, startSession, endSession } from './utils/analytics';
import {
  Mic,
  Upload,
  Cpu,
  Monitor,
  Video,
  Share2,
  Check,
} from 'lucide-react';

/**
 * TIWATON AUDIO AI
 * UPDATE:
 * - Hyper-Gate + VAD
 * - File Upload mode (audio + video audio track)
 * - Smart Gain Rider after compressor (driven by RMS)
 * - Gain Rider readout in footer
 * - Speech Priority Boost visual + label
 * - Process & Export for File mode (AI-mastered audio)
 */

// In a browser the website at the root is home (src/site/); the desktop app has no home key.
const goToSite = () => window.location.assign('/');

const TiwatonApp = () => <AudioProcessor goHome={isDesktopApp ? null : goToSite} />;

// --- STUDIO COMPONENT ---
const DEFAULT_DEVICES = {
  inputId: 'default',
  outputId: 'default',
  broadcastBus: 'Not set',
  mainOutputId: 'Not set',
  monitor2OutputId: 'Not set',
  monitor3OutputId: 'Not set',
};

const MAIN_TABS = [
  { id: 'live', label: 'live' },
  { id: 'editor', label: 'editor' },
  { id: 'mixer', label: 'mixer' },
];

const AudioProcessor = ({ goHome }) => {
  const [isLive, setIsLive] = useState(false);
  const [isPlayingFile, setIsPlayingFile] = useState(false);
  const [audioContext, setAudioContext] = useState(null);
  const [mode, setMode] = useState('live');

  // OLD single "outputTarget" (display of which streaming app you're sending to)
  const [outputTarget, setOutputTarget] = useState('OBS Studio');

  const [showSettings, setShowSettings] = useState(false);
  const [showLink, setShowLink] = useState(false);
  const [showObs, setShowObs] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  // Desktop: the monitor choice is remembered between sessions. Browser: off at start, to avoid
  // speaker feedback with a laptop mic.
  const [isMonitoring, setIsMonitoring] = useState(() => {
    if (!isTauri) return false;
    try { return readMonitorPref(window.localStorage.getItem(MONITOR_PREF_KEY)); } catch { return false; }
  });
  const [isBypassed, setIsBypassed] = useState(false);
  const [exportStatus, setExportStatus] = useState(null);
  const [inputGainValue, setInputGainValue] = useState(1.0);
  const [voiceActive, setVoiceActive] = useState(false);
  const [isAutoCalibrating, setIsAutoCalibrating] = useState(false);
  const [audioEngineError, setAudioEngineError] = useState(null);
  // True while the desktop engine is opening its devices, so "start" shows it is working.
  const [engineStarting, setEngineStarting] = useState(false);
  const [chainReady, setChainReady] = useState(0); // increments when audio chain is fully built
  const [mainTab, setMainTab] = useState('live'); // 'live' | 'editor' | 'mixer'
  // Mixer console: real Tauri controller (invoke/listen loaded lazily, same
  // pattern as the native engine helpers above) when running under the
  // desktop app, otherwise a fully in-memory demo controller in the browser.
  const mixerController = useMemo(() => {
    if (isTauri) {
      const invoke = (command, payload) => (
        import('@tauri-apps/api/core').then(({ invoke: tauriInvoke }) => tauriInvoke(command, payload))
      );
      const listen = (event, cb) => (
        import('@tauri-apps/api/event').then(({ listen: tauriListen }) => tauriListen(event, cb))
      );
      return createMixerController({ invoke, listen });
    }
    return createMockMixerController();
  }, []);
  const [gateMode, setGateMode] = useState(() => {
    return window.localStorage.getItem('tiwaton:gateMode') || 'balanced';
  });

  const [noiseFloorThreshold, setNoiseFloorThreshold] = useState(() => {
    const saved = window.localStorage.getItem('tiwaton:threshold');
    return saved ? parseFloat(saved) : -50;
  });
  const [visualizerGateStatus, setVisualizerGateStatus] = useState(false);
  const [recordingState, setRecordingState] = useState('idle');
  const [recordedUrl, setRecordedUrl] = useState(null);
  const [recordingDuration, setRecordingDuration] = useState(0);
  const [gainRiderDb, setGainRiderDb] = useState(0); // Smart Gain Rider readout

  // Negotiated once per session: which video container MediaRecorder will
  // actually produce here. Drives both the download extension and every
  // "MP4"/"WebM" label in the UI, so we never call a WebM file "MP4".
  const videoFormat = useMemo(
    () =>
      pickVideoMimeType(
        typeof MediaRecorder !== 'undefined'
          ? MediaRecorder.isTypeSupported.bind(MediaRecorder)
          : undefined,
      ),
    [],
  );

  // playback tracking for record-check review
  const [playbackPosition, setPlaybackPosition] = useState(0);
  const [playbackDuration, setPlaybackDuration] = useState(0);
  const [isPlayingBack, setIsPlayingBack] = useState(false);
  const playbackAudioRef = useRef(null);

  // File mode info + export state
  const [fileInfo, setFileInfo] = useState(null); // { name, type, isVideo }
  const [fileExportStatus, setFileExportStatus] = useState(null);

  // File editing (trim & timing)
  const [fileDuration, setFileDuration] = useState(0);
  const [trimStart, setTrimStart] = useState(0);
  const [trimEnd, setTrimEnd] = useState(0);
  const [isPreviewingTrim, setIsPreviewingTrim] = useState(false);

  const [availableDevices, setAvailableDevices] = useState({
    inputs: [],
    outputs: [],
  });

  // UPDATED: selectedDevices now holds:
  // - inputId           → mic / mixer
  // - outputId          → monitoring output (AirPods, speakers) – used by setSinkId
  // - broadcastBus      → conceptual "broadcast output" (VB-Cable, Loopback, etc)
  const [selectedDevices, setSelectedDevices] = useState(() => {
    try {
      const saved = window.localStorage.getItem('tiwaton:devices');
      return { ...DEFAULT_DEVICES, ...(saved ? JSON.parse(saved) : {}) };
    } catch { return { ...DEFAULT_DEVICES }; }
  });

  const [audioStats, setAudioStats] = useState({
    sampleRate: 0,
    bufferSize: 0,
    latencyMs: null,
    callbackAvgMs: null,
    callbackPeakMs: null,
    cpuLoadPct: null,
    droppedOutputSamples: 0,
    state: 'suspended',
  });

  // -- Refs --
  const contextRef = useRef(null);
  const streamRef = useRef(null);
  const audioElRef = useRef(null);
  const videoElRef = useRef(null);
  const fileInputRef = useRef(null);
  const mediaRecorderRef = useRef(null);
  const recordedChunksRef = useRef([]);
  const recordingTimerRef = useRef(null);
  const destNodeRef = useRef(null);

  // -- Processing Refs --
  const processingRefs = useRef({
    source: null,
    inputGain: null,
    lowCut: null,
    gateGain: null,
    deEsser: null,
    compressor: null,
    autoGain: null, // Smart Gain Rider node
    eqWarmth: null,
    eqClarity: null,
    reverbFilter: null,
    reverbDucker: null,
    noiseNotch: null,
    voiceContour: null,
    noiseBlend: null,
    noiseLowShelf: null,
    noiseHighShelf: null,
    duckGain: null,
    polishLow: null,
    polishPresence: null,
    polishAir: null,
    limiter: null,
    master: null,
    monitorGain: null,
    analyser: null,
  });

  // -- Logic Refs --
  const settingsRef = useRef({
    denoise: false,
    threshold: -50,
    isBypassed: false,
  });
  const isMonitoringRef = useRef(isMonitoring);

  const calibrateRef = useRef({
    active: false,
    startedAt: 0,
    sumSq: 0,
    frames: 0,
  });

  // --- Mic Fingerprint System ---
  // Learns the spectral "signature" of the connected mic over time.
  // Anything that deviates from the learned profile is likely ambient noise (TV, kids, etc).
  const micFingerprintRef = useRef({
    learning: false,
    learned: false,
    frames: 0,
    // Running mean of energy per band (8 bands spanning 40-12000Hz)
    bandMeans: new Float32Array(8),
    bandM2: new Float32Array(8), // for variance (Welford's algorithm)
    // Spectral centroid mean/variance (tracks "center of gravity" of the mic's sound)
    centroidMean: 0,
    centroidM2: 0,
    // How many frames to learn (5 seconds at ~100fps = 500 frames)
    learnFrames: 500,
  });

  // -- Features --
  const [features, setFeatures] = useState(() => {
    try {
      const saved = window.localStorage.getItem('tiwaton:features');
      return saved ? JSON.parse(saved) : {
        denoise: false, dereverb: false, pastorIsolation: false,
        sermonWarmth: false, smartMixing: false, mastering: false,
        voicePattern: false, musicDucking: false, streamingSafe: false,
        lookaheadGate: false, dynamicDeEsser: true, multibandComp: false,
      };
    } catch {
      return {
        denoise: false, dereverb: false, pastorIsolation: false,
        sermonWarmth: false, smartMixing: false, mastering: false,
        voicePattern: false, musicDucking: false, streamingSafe: false,
        lookaheadGate: false, dynamicDeEsser: true, multibandComp: false,
      };
    }
  });
  const [speakerPreset, setSpeakerPreset] = useState(() => {
    return window.localStorage.getItem('tiwaton:speakerPreset') || 'balanced';
  });
  const [loudnessDb, setLoudnessDb]     = useState(null);
  const [loudnessTarget, setLoudnessTarget] = useState(-14);
  // True LUFS metering (ITU-R BS.1770) — set by LUFS worklet
  const [lufsM,  setLufsM]  = useState(null); // momentary (400ms)
  const [lufsST, setLufsST] = useState(null); // short-term (3s)
  const [lufsI,  setLufsI]  = useState(null); // integrated (gated)
  // De-esser gain reduction for meter
  const [desserGrDb, setDesserGrDb] = useState(0);
  const [abMode, setAbMode] = useState('B');
  const [snapshots, setSnapshots] = useState(() => {
    try { return parseSnapshots(window.localStorage.getItem(SNAPSHOTS_KEY)); } catch { return []; }
  });
  const snapshotCounterRef = useRef(nextSnapshotId(snapshots));
  const [noiseProfiles, setNoiseProfiles] = useState(() => {
    try {
      const saved = window.localStorage.getItem('tiwaton:noiseProfiles');
      return saved ? JSON.parse(saved) : [];
    } catch { return []; }
  });
  const [selectedNoiseProfileId, setSelectedNoiseProfileId] = useState(() => {
    return window.localStorage.getItem('tiwaton:selectedNoiseProfileId') || '';
  });
  const [nativeNoiseProfileCaptured, setNativeNoiseProfileCaptured] = useState(false);
  const [nativeRouteStatus, setNativeRouteStatus] = useState(null);
  // Whether computer-audio capture has muted the tapped apps at the speakers (macOS).
  const [computerMutesSpeakers, setComputerMutesSpeakers] = useState(false);
  const [roomName, setRoomName] = useState(() => {
    return window.localStorage.getItem('tiwaton:roomName') || 'Main Sanctuary';
  });
  const noiseProfileCounterRef = useRef(nextNoiseProfileNumber(noiseProfiles));
  const latestSpectrumRef = useRef(null);
  const noiseFloorThresholdRef = useRef(noiseFloorThreshold);
  const featuresRef = useRef(features);
  const gateModeRef = useRef(gateMode);
  const noiseProfilesRef = useRef(noiseProfiles);
  const selectedNoiseProfileIdRef = useRef(selectedNoiseProfileId);
  const gainRiderDbRef = useRef(gainRiderDb);
  const canvasRef = useRef(null);
  const animationRef = useRef(null);
  // Tauri Rust engine — spectrum data injected from audio-meters event
  const tauriSpectrumRef = useRef(new Float32Array(128));
  const tauriUnlistenRef = useRef(null);

  useEffect(() => {
    isMonitoringRef.current = isMonitoring;
  }, [isMonitoring]);

  useEffect(() => {
    featuresRef.current = features;
    try { window.localStorage.setItem('tiwaton:features', JSON.stringify(features)); } catch { /* Ignore local persistence failures. */ }
  }, [features]);

  // "Stream safe" and the loudness target drive the native Stream bus leveler.
  const streamSafeOn = Boolean(features.streamingSafe);
  useEffect(() => {
    void syncStreamLeveler(mixerController, { on: streamSafeOn, target: loudnessTarget }).then((error) => {
      if (error) console.warn('[TIWATON] Stream leveler not updated:', error);
    });
  }, [mixerController, streamSafeOn, loudnessTarget]);

  useEffect(() => {
    gateModeRef.current = gateMode;
    try { window.localStorage.setItem('tiwaton:gateMode', gateMode); } catch { /* Ignore local persistence failures. */ }
  }, [gateMode]);

  useEffect(() => {
    noiseProfilesRef.current = noiseProfiles;
    try { window.localStorage.setItem('tiwaton:noiseProfiles', JSON.stringify(noiseProfiles)); } catch { /* Ignore local persistence failures. */ }
  }, [noiseProfiles]);

  // Voice snapshots are the operator's saved setups: they outlive the app and "reset".
  useEffect(() => {
    try { window.localStorage.setItem(SNAPSHOTS_KEY, serializeSnapshots(snapshots)); } catch { /* Ignore local persistence failures. */ }
  }, [snapshots]);

  useEffect(() => {
    selectedNoiseProfileIdRef.current = selectedNoiseProfileId;
    try { window.localStorage.setItem('tiwaton:selectedNoiseProfileId', selectedNoiseProfileId); } catch { /* Ignore local persistence failures. */ }
  }, [selectedNoiseProfileId]);

  useEffect(() => {
    try { window.localStorage.setItem('tiwaton:roomName', roomName); } catch { /* Ignore local persistence failures. */ }
  }, [roomName]);

  useEffect(() => {
    gainRiderDbRef.current = gainRiderDb;
  }, [gainRiderDb]);

  useEffect(() => {
    try { window.localStorage.setItem('tiwaton:devices', JSON.stringify(selectedDevices)); } catch { /* Ignore local persistence failures. */ }
  }, [selectedDevices]);

  useEffect(() => {
    try { window.localStorage.setItem('tiwaton:speakerPreset', speakerPreset); } catch { /* Ignore local persistence failures. */ }
  }, [speakerPreset]);

  const outputTargets = [
    { name: 'OBS Studio', icon: <Cpu size={20} /> },
    { name: 'vMix Live', icon: <Monitor size={20} /> },
    { name: 'YouTube Live', icon: <Share2 size={20} /> },
    { name: 'Facebook Live', icon: <Share2 size={20} /> },
    { name: 'Zoom / Teams', icon: <Video size={20} /> },
  ];

  const cycleOutputTarget = () => {
    const currentIndex = outputTargets.findIndex((t) => t.name === outputTarget);
    const nextIndex = (currentIndex + 1) % outputTargets.length;
    setOutputTarget(outputTargets[nextIndex].name);
  };

  const resetDeviceState = useCallback(() => {
    setAvailableDevices({ inputs: [], outputs: [] });
    setSelectedDevices({ ...DEFAULT_DEVICES });
  }, []);

  const getDevices = useCallback(async () => {
    try {
      let nextDevices;
      if (isTauri) {
        const { invoke } = await import('@tauri-apps/api/core');
        const [inputs, outputs] = await Promise.all([
          invoke('list_input_devices'),
          invoke('list_output_devices'),
        ]);
        nextDevices = {
          inputs: normalizeTauriDevices(inputs),
          outputs: normalizeTauriDevices(outputs),
        };
      } else {
        await navigator.mediaDevices.getUserMedia({ audio: true });
        const devices = await navigator.mediaDevices.enumerateDevices();
        nextDevices = {
          inputs: devices.filter((d) => d.kind === 'audioinput'),
          outputs: devices.filter((d) => d.kind === 'audiooutput'),
        };
      }

      setAvailableDevices(nextDevices);
      return nextDevices;
    } catch (err) {
      console.warn('Permission needed', err);
      resetDeviceState();
      return { inputs: [], outputs: [] };
    }
  }, [resetDeviceState]);

  useEffect(() => {
    setSelectedDevices((prev) => {
      const next = reconcileSelectedDevices(prev, availableDevices);
      return devicesEqual(next, prev) ? prev : next;
    });
  }, [availableDevices]);

  useEffect(() => {
    if (showSettings) { getDevices(); trackEvent('settings_open'); }
  }, [showSettings, getDevices]);

  useEffect(() => {
    if (isTauri) {
      getDevices();
    }
  }, [getDevices]);

  // Session-level analytics
  useEffect(() => {
    startSession();
    return () => endSession();
  }, []);

  useEffect(() => {
    noiseFloorThresholdRef.current = noiseFloorThreshold;
    try { window.localStorage.setItem('tiwaton:threshold', String(noiseFloorThreshold)); } catch { /* Ignore local persistence failures. */ }
  }, [noiseFloorThreshold]);

  useEffect(() => {
    settingsRef.current = {
      denoise: features.denoise,
      threshold: noiseFloorThreshold,
      isBypassed: isBypassed
    };

    // Sync HyperGate AudioWorklet with current settings + mic fingerprint
    const hgNode = processingRefs.current.hyperGateNode;
    if (hgNode) {
      hgNode.port.postMessage({
        type: 'config',
        threshold: noiseFloorThreshold,
        gateMode: gateMode,
        enabled: features.denoise,
        bypassed: isBypassed,
      });
      // Push fingerprint to worklet whenever it's learned
      const fp = micFingerprintRef.current;
      if (fp.learned && fp.bandMeans) {
        hgNode.port.postMessage({
          type: 'fingerprint',
          bandMeans: Array.from(fp.bandMeans),
          bandStdDevs: Array.from(fp.bandM2).map((m2) =>
            fp.frames > 1 ? Math.sqrt(m2 / (fp.frames - 1)) : 0.5
          ),
        });
      }
    }

    // Sync RNNoise enable/disable via a dry/wet swap (the real
    // RnnoiseWorkletNode has no port-based enable message). The wet path
    // lags the dry one by ~11 ms, so the overlap is kept to 4 ms: long
    // enough to avoid a click, too short to hear as comb filtering.
    const rnDry = processingRefs.current.rnnoiseDryGain;
    const rnWet = processingRefs.current.rnnoiseWetGain;
    if (rnDry && rnWet) {
      const wantRnnoise = !isBypassed && (features.voicePattern || features.denoise);
      const now = audioContext?.currentTime || 0;
      const end = now + 0.004;
      for (const [param, target] of [[rnDry.gain, wantRnnoise ? 0 : 1], [rnWet.gain, wantRnnoise ? 1 : 0]]) {
        param.cancelScheduledValues(now);
        param.setValueAtTime(param.value, now);
        param.linearRampToValueAtTime(target, end);
      }
    }

    // Force open gate if disabled
    if (!features.denoise && processingRefs.current.gateGain) {
      processingRefs.current.gateGain.gain.setValueAtTime(
        1.0,
        audioContext?.currentTime || 0
      );
    }
  }, [features.denoise, features.voicePattern, noiseFloorThreshold, gateMode, isBypassed, audioContext]);

  const stopTauriEngine = useCallback(async () => {
    if (!isTauri) return;
    const { invoke } = await import('@tauri-apps/api/core');
    await stopNativeEngine(invoke);
  }, []);

  const cleanupAudio = useCallback(({ stopNative = true } = {}) => {
    // Free RNNoise's WASM-side state explicitly; closing the context alone
    // leaves that to the browser.
    try {
      processingRefs.current.rnnoiseNode?.destroy?.();
    } catch (e) {
      console.warn('Error destroying RNNoise node', e);
    }
    if (contextRef.current) {
      try {
        contextRef.current.close();
      } catch (e) {
        console.warn('Error closing audio context', e);
      }
      contextRef.current = null;
    }
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
    }
    if (animationRef.current) cancelAnimationFrame(animationRef.current);

    // Stop Rust audio engine (Tauri mode)
    if (tauriUnlistenRef.current) {
      tauriUnlistenRef.current();
      tauriUnlistenRef.current = null;
    }
    if (stopNative && isTauri) {
      stopTauriEngine().catch(() => {});
    }

    // stop media elements
    if (audioElRef.current) {
      audioElRef.current.pause();
      audioElRef.current.src = '';
    }
    if (videoElRef.current) {
      videoElRef.current.pause();
      videoElRef.current.src = '';
    }

      processingRefs.current = {
        source: null,
        inputGain: null,
        lowCut: null,
        gateGain: null,
        deEsser: null,
        compressor: null,
        autoGain: null,
        eqWarmth: null,
        eqClarity: null,
        reverbFilter: null,
        reverbDucker: null,
        noiseNotch: null,
        voiceContour: null,
        noiseBlend: null,
        noiseLowShelf: null,
        noiseHighShelf: null,
        duckGain: null,
        polishLow: null,
        polishPresence: null,
        polishAir: null,
        limiter: null,
        master: null,
        monitorGain: null,
        analyser: null,
      };
    destNodeRef.current = null;

    setAudioContext(null);
    setIsLive(false);
    setIsPlayingFile(false);
    setAudioStats({
      sampleRate: 0,
      bufferSize: 0,
      latencyMs: null,
      callbackAvgMs: null,
      callbackPeakMs: null,
      cpuLoadPct: null,
      droppedOutputSamples: 0,
      state: 'suspended',
    });
    setExportStatus(null);
    setFileExportStatus(null);
    setFileInfo(null);

    // reset trim editor
    setFileDuration(0);
    setTrimStart(0);
    setTrimEnd(0);
    setIsPreviewingTrim(false);

    if (recordingTimerRef.current) clearInterval(recordingTimerRef.current);
    setRecordingState('idle');
    setVoiceActive(false);
    setVisualizerGateStatus(false);
    setGainRiderDb(0);
    setNativeNoiseProfileCaptured(false);
    setNativeRouteStatus(null);
    setAudioEngineError(null);
  }, [stopTauriEngine]);

  useEffect(() => {
    return () => cleanupAudio();
  }, [cleanupAudio]);

  const hardReset = () => {
    cleanupAudio();

    setMode('live');
    setIsBypassed(false);
    setIsMonitoring(false);
    setFeatures({
      denoise: false,
      dereverb: false,
      pastorIsolation: false,
      sermonWarmth: false,
      smartMixing: false,
      mastering: false,
      voicePattern: false,
      musicDucking: false,
      streamingSafe: false,
      lookaheadGate: false,
      dynamicDeEsser: true,
      multibandComp: false,
    });
    setSpeakerPreset('balanced');
    setLoudnessDb(null);
    setLufsM(null); setLufsST(null); setLufsI(null);
    setLoudnessTarget(-14);
    setAbMode('B');
    setNoiseProfiles([]);
    setSelectedNoiseProfileId('');
    noiseProfileCounterRef.current = 1;

    setNoiseFloorThreshold(-50);
    setGainRiderDb(0);
    setVoiceActive(false);
    setVisualizerGateStatus(false);

    setRecordingState('idle');
    setRecordingDuration(0);
    setRecordedUrl(null);
    setExportStatus(null);

    setFileInfo(null);
    setFileExportStatus(null);
    setPlaybackPosition(0);
    setPlaybackDuration(0);
    setIsPlayingBack(false);

    resetDeviceState();

    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  // Read by the audio-graph builder, which can run from an older render.
  const isBypassedRef = useRef(isBypassed);
  useEffect(() => {
    isBypassedRef.current = isBypassed;
    setAbMode(isBypassed ? 'A' : 'B');
  }, [isBypassed]);

  const startAudioEngine = async (inputStream = null, mediaElement = null) => {
    try {
      // Reuse existing context if it's already open
      if (contextRef.current && contextRef.current.state !== 'closed') {
        setAudioEngineError(null);
        visualizeAndGate();
        return;
      }

      const Ctor = window.AudioContext || window.webkitAudioContext;
      if (!Ctor) {
        throw new Error(
          'Web Audio is not supported in this browser. Try Chrome, Edge, or a modern browser.',
        );
      }

      // RNNoise (neural denoise) assumes a 48kHz pipeline, and our offline
      // export contexts already render at 48000 — request it here too so
      // the live engine and exports agree and RNNoise can actually run.
      let ctx;
      try {
        ctx = new Ctor({ sampleRate: 48000 });
      } catch (rateErr) {
        console.warn('[TIWATON] Could not force 48kHz AudioContext, using device default:', rateErr.message);
        ctx = new Ctor();
      }

      if (ctx.state === 'suspended') {
        await ctx.resume();
      }

      contextRef.current = ctx;
      setAudioContext(ctx);
      setAudioStats({
        sampleRate: ctx.sampleRate,
        bufferSize: 128,
        latencyMs: null,
        callbackAvgMs: null,
        callbackPeakMs: null,
        cpuLoadPct: null,
        droppedOutputSamples: 0,
        state: ctx.state,
      });

      // --- Load AudioWorklet processors ---
      let hyperGateNode = null;
      let rnnoiseNode = null;

      try {
        await ctx.audioWorklet.addModule('/worklets/hypergate-worklet.js');
        hyperGateNode = new AudioWorkletNode(ctx, 'hypergate-processor');

        // Send initial config
        hyperGateNode.port.postMessage({
          type: 'config',
          threshold: noiseFloorThresholdRef.current ?? -50,
          gateMode: gateModeRef.current,
          enabled: featuresRef.current.denoise,
          bypassed: false,
        });

        // Receive metrics from worklet for UI
        hyperGateNode.port.onmessage = (event) => {
          const msg = event.data;
          if (msg.type === 'metrics') {
            if (typeof msg.gateClosed === 'boolean') {
              setVisualizerGateStatus((prev) => (prev !== msg.gateClosed ? msg.gateClosed : prev));
            }
          }
        };

        console.log('[TIWATON] HyperGate AudioWorklet loaded (sub-1ms latency)');
      } catch (err) {
        console.warn('[TIWATON] AudioWorklet not available, falling back to main-thread gate:', err.message);
      }

      if (ctx.sampleRate !== 48000) {
        console.warn(
          `[TIWATON] RNNoise needs a 48kHz AudioContext, but this one is running at ${ctx.sampleRate}Hz — skipping neural denoise.`,
        );
      } else {
        try {
          const { wasmBinary, RnnoiseWorkletNode, workletUrl } = await loadRnnoiseAssets();
          await ctx.audioWorklet.addModule(workletUrl);
          rnnoiseNode = new RnnoiseWorkletNode(ctx, { wasmBinary, maxChannels: 1 });
          console.log('[TIWATON] RNNoise WASM loaded (neural noise suppression active)');
        } catch (err) {
          console.warn('[TIWATON] RNNoise unavailable, continuing without neural denoise:', err.message);
          rnnoiseNode = null;
        }
      }

      // ── LUFS Meter (ITU-R BS.1770) ────────────────────────────────────────
      let lufsNode = null;
      try {
        await ctx.audioWorklet.addModule('/worklets/lufs-worklet.js');
        lufsNode = new AudioWorkletNode(ctx, 'lufs-processor', { numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [1] });
        lufsNode.port.onmessage = (e) => {
          const { momentary, shortTerm, integrated } = e.data;
          setLufsM( parseFloat(momentary.toFixed(1)));
          setLufsST(parseFloat(shortTerm.toFixed(1)));
          setLufsI( parseFloat(integrated.toFixed(1)));
          // Keep legacy loudnessDb in sync for streamingSafe guard
          setLoudnessDb(parseFloat(shortTerm.toFixed(1)));
        };
        console.log('[TIWATON] LUFS Meter loaded (ITU-R BS.1770-4)');
      } catch (err) {
        console.warn('[TIWATON] LUFS worklet unavailable:', err.message);
      }

      // ── Lookahead Gate ────────────────────────────────────────────────────
      let lookaheadGateNode = null;
      try {
        await ctx.audioWorklet.addModule('/worklets/lookahead-gate-worklet.js');
        lookaheadGateNode = new AudioWorkletNode(ctx, 'lookahead-gate-processor');
        lookaheadGateNode.parameters.get('enabled').value =
          featuresRef.current.lookaheadGate ? 1 : 0;
        console.log('[TIWATON] Lookahead Gate loaded (20ms attack-safe gating)');
      } catch (err) {
        console.warn('[TIWATON] Lookahead gate worklet unavailable:', err.message);
      }

      // ── Dynamic De-esser ─────────────────────────────────────────────────
      let dynamicDesserNode = null;
      try {
        await ctx.audioWorklet.addModule('/worklets/dynamic-desser-worklet.js');
        dynamicDesserNode = new AudioWorkletNode(ctx, 'dynamic-desser-processor');
        dynamicDesserNode.parameters.get('enabled').value =
          featuresRef.current.dynamicDeEsser ? 1 : 0;
        dynamicDesserNode.port.onmessage = (e) => {
          if (typeof e.data.grDb === 'number') setDesserGrDb(parseFloat(e.data.grDb.toFixed(1)));
        };
        console.log('[TIWATON] Dynamic De-esser loaded (sidechain sibilance detector)');
      } catch (err) {
        console.warn('[TIWATON] Dynamic de-esser worklet unavailable:', err.message);
      }

      // ── Spectral Dereverb ─────────────────────────────────────────────────
      let dereverbNode = null;
      try {
        await ctx.audioWorklet.addModule('/worklets/dereverb-worklet.js');
        dereverbNode = new AudioWorkletNode(ctx, 'dereverb-processor');
        dereverbNode.port.postMessage({
          enabled:  featuresRef.current.dereverb,
          strength: 1.5,
          floor:    0.08,
        });
        console.log('[TIWATON] Spectral Dereverb loaded (overlap-add minimum-statistics)');
      } catch (err) {
        console.warn('[TIWATON] Dereverb worklet unavailable:', err.message);
      }

      let source;

      if (inputStream) {
        const hasAudioTracks =
          typeof inputStream.getAudioTracks === 'function' &&
          inputStream.getAudioTracks().length > 0;

        if (!hasAudioTracks) {
          throw new Error(
            'Selected input has no audio track. Check your microphone / audio device.',
          );
        }

        source = ctx.createMediaStreamSource(inputStream);
      } else if (mediaElement) {
        source = ctx.createMediaElementSource(mediaElement);
      } else {
        throw new Error('No input stream or media element provided to audio engine.');
      }

      // --- DSP NODES ---
      const inputGain = ctx.createGain();
      inputGain.gain.value = inputGainValue;

      const lowCut = ctx.createBiquadFilter();
      lowCut.type = 'highpass';
      lowCut.frequency.value = 80;

      const gateGain = ctx.createGain();
      gateGain.gain.value = 1.0;

      const noiseNotch = ctx.createBiquadFilter();
      noiseNotch.type = 'notch';
      noiseNotch.frequency.value = 80;
      noiseNotch.Q.value = 1;

      const voiceContour = ctx.createBiquadFilter();
      voiceContour.type = 'peaking';
      voiceContour.frequency.value = 1500;
      voiceContour.Q.value = 0.7;
      voiceContour.gain.value = 0;

      const noiseBlend = ctx.createGain();
      noiseBlend.gain.value = 1.0;

      const noiseLowShelf = ctx.createBiquadFilter();
      noiseLowShelf.type = 'lowshelf';
      noiseLowShelf.frequency.value = 180;
      noiseLowShelf.gain.value = 0;

      const noiseHighShelf = ctx.createBiquadFilter();
      noiseHighShelf.type = 'highshelf';
      noiseHighShelf.frequency.value = 7500;
      noiseHighShelf.gain.value = 0;

      const deEsser = ctx.createBiquadFilter();
      deEsser.type = 'peaking';
      deEsser.frequency.value = 7000;
      deEsser.Q.value = 1;
      deEsser.gain.value = 0;

      const compressor = ctx.createDynamicsCompressor();
      compressor.threshold.value = -24;
      compressor.knee.value = 30;
      compressor.ratio.value = 4;
      compressor.attack.value = 0.003;
      compressor.release.value = 0.25;

      const autoGain = ctx.createGain();
      autoGain.gain.value = 1.0; // Smart Gain Rider moves this

      const eqWarmth = ctx.createBiquadFilter();
      eqWarmth.type = 'peaking';
      eqWarmth.frequency.value = 250;
      eqWarmth.Q.value = 1;
      eqWarmth.gain.value = 0;

      const eqClarity = ctx.createBiquadFilter();
      eqClarity.type = 'peaking';
      eqClarity.frequency.value = 3500;
      eqClarity.Q.value = 1;
      eqClarity.gain.value = 0;

      const reverbFilter = ctx.createBiquadFilter();
      reverbFilter.type = 'lowpass';
      reverbFilter.frequency.value = 18000;
      reverbFilter.Q.value = 0.7;

      const reverbDucker = ctx.createGain();
      reverbDucker.gain.value = 1.0;
      const duckGain = ctx.createGain();
      duckGain.gain.value = 1.0;

      const polishLow = ctx.createBiquadFilter();
      polishLow.type = 'lowshelf';
      polishLow.frequency.value = 180;
      polishLow.gain.value = 0;

      const polishPresence = ctx.createBiquadFilter();
      polishPresence.type = 'peaking';
      polishPresence.frequency.value = 3200;
      polishPresence.Q.value = 0.9;
      polishPresence.gain.value = 0;

      const polishAir = ctx.createBiquadFilter();
      polishAir.type = 'highshelf';
      polishAir.frequency.value = 9000;
      polishAir.gain.value = 0;

      const limiter = ctx.createDynamicsCompressor();
      limiter.threshold.value = -1;
      limiter.knee.value = 0;
      limiter.ratio.value = 20;
      limiter.attack.value = 0.001;
      limiter.release.value = 0.1;

      const master = ctx.createGain();
      master.gain.value = 1.0;

      const ana = ctx.createAnalyser();
      ana.fftSize = 2048;

      const monitorGain = ctx.createGain();
      monitorGain.gain.value = isMonitoringRef.current ? 1.0 : 0.0;

      const destNode = ctx.createMediaStreamDestination();
      destNodeRef.current = destNode;

      // ── Multiband Compressor (4 bands: Sub / Low-Mid / Mid / Air) ─────────
      // Each band: crossover filter(s) → DynamicsCompressor → GainNode
      // All band outputs connect to mbMerge (auto-summed by Web Audio)
      const mbMerge = ctx.createGain(); mbMerge.gain.value = 1.0;

      // Sub band (0–150 Hz) — controls low rumble / proximity effect
      const mb_sub_lp = ctx.createBiquadFilter();
      mb_sub_lp.type = 'lowpass'; mb_sub_lp.frequency.value = 150; mb_sub_lp.Q.value = 0.7;
      const mb_sub_comp = ctx.createDynamicsCompressor();
      mb_sub_comp.threshold.value = -20; mb_sub_comp.knee.value = 6;
      mb_sub_comp.ratio.value = 3; mb_sub_comp.attack.value = 0.02; mb_sub_comp.release.value = 0.3;
      const mb_sub_gain = ctx.createGain(); mb_sub_gain.gain.value = 1.0;

      // Low-mid band (150–800 Hz) — controls body / warmth
      const mb_lm_hp = ctx.createBiquadFilter();
      mb_lm_hp.type = 'highpass'; mb_lm_hp.frequency.value = 150; mb_lm_hp.Q.value = 0.7;
      const mb_lm_lp = ctx.createBiquadFilter();
      mb_lm_lp.type = 'lowpass'; mb_lm_lp.frequency.value = 800; mb_lm_lp.Q.value = 0.7;
      const mb_lm_comp = ctx.createDynamicsCompressor();
      mb_lm_comp.threshold.value = -22; mb_lm_comp.knee.value = 8;
      mb_lm_comp.ratio.value = 3.5; mb_lm_comp.attack.value = 0.01; mb_lm_comp.release.value = 0.2;
      const mb_lm_gain = ctx.createGain(); mb_lm_gain.gain.value = 1.0;

      // Mid band (800 Hz–4 kHz) — controls presence / intelligibility (most critical for speech)
      const mb_mid_hp = ctx.createBiquadFilter();
      mb_mid_hp.type = 'highpass'; mb_mid_hp.frequency.value = 800; mb_mid_hp.Q.value = 0.7;
      const mb_mid_lp = ctx.createBiquadFilter();
      mb_mid_lp.type = 'lowpass'; mb_mid_lp.frequency.value = 4000; mb_mid_lp.Q.value = 0.7;
      const mb_mid_comp = ctx.createDynamicsCompressor();
      mb_mid_comp.threshold.value = -18; mb_mid_comp.knee.value = 6;
      mb_mid_comp.ratio.value = 4; mb_mid_comp.attack.value = 0.005; mb_mid_comp.release.value = 0.15;
      const mb_mid_gain = ctx.createGain(); mb_mid_gain.gain.value = 1.0;

      // Air band (4 kHz+) — controls brilliance / air / sibilance
      const mb_air_hp = ctx.createBiquadFilter();
      mb_air_hp.type = 'highpass'; mb_air_hp.frequency.value = 4000; mb_air_hp.Q.value = 0.7;
      const mb_air_comp = ctx.createDynamicsCompressor();
      mb_air_comp.threshold.value = -24; mb_air_comp.knee.value = 10;
      mb_air_comp.ratio.value = 2.5; mb_air_comp.attack.value = 0.002; mb_air_comp.release.value = 0.1;
      const mb_air_gain = ctx.createGain(); mb_air_gain.gain.value = 1.0;

      // Gate output node — the signal entering the DSP chain after gating
      const gateOutput = ctx.createGain(); gateOutput.gain.value = 1.0;

      // ════ SIGNAL CHAIN ════════════════════════════════════════════════════
      source.connect(inputGain);
      inputGain.connect(lowCut);

      // ── Neural Denoise (RNNoise WASM) ────────────────────────────────────
      // RnnoiseWorkletNode has no built-in enable/disable message, so the
      // "denoise" toggle is implemented as a dry/wet gain crossfade around
      // it instead — both paths run, only the audible gain changes.
      let rnnoiseDryGain = null;
      let rnnoiseWetGain = null;
      if (rnnoiseNode) {
        rnnoiseDryGain = ctx.createGain();
        rnnoiseWetGain = ctx.createGain();
        // BYPASS AI means raw audio, so it selects the dry path too.
        const wantRnnoise = !isBypassedRef.current
          && (featuresRef.current.voicePattern || featuresRef.current.denoise);
        rnnoiseDryGain.gain.value = wantRnnoise ? 0 : 1;
        rnnoiseWetGain.gain.value = wantRnnoise ? 1 : 0;

        lowCut.connect(rnnoiseDryGain);
        rnnoiseDryGain.connect(ana);

        lowCut.connect(rnnoiseNode);
        rnnoiseNode.connect(rnnoiseWetGain);
        rnnoiseWetGain.connect(ana);
      } else {
        lowCut.connect(ana);
      }

      // ── Gate Layer (HyperGate or Lookahead Gate or GainNode fallback) ────
      // Lookahead gate: delays signal 20ms and pre-opens gate before syllables
      // HyperGate: FFT-based voice detection, sub-1ms reaction
      // Both can coexist: lookahead gate first (delays signal), then HyperGate re-gates
      if (lookaheadGateNode) {
        ana.connect(lookaheadGateNode);
        if (hyperGateNode) {
          lookaheadGateNode.connect(hyperGateNode);
          hyperGateNode.connect(gateOutput);
        } else {
          lookaheadGateNode.connect(gateOutput);
        }
      } else if (hyperGateNode) {
        ana.connect(hyperGateNode);
        hyperGateNode.connect(gateOutput);
      } else {
        ana.connect(gateGain);
        gateGain.connect(gateOutput);
      }

      // ── EQ + Spectral Shaping ─────────────────────────────────────────────
      gateOutput.connect(noiseNotch);
      noiseNotch.connect(voiceContour);
      voiceContour.connect(noiseBlend);
      noiseBlend.connect(noiseLowShelf);
      noiseLowShelf.connect(noiseHighShelf);

      // ── Dynamic De-esser (sidechain sibilance detector) ──────────────────
      // Sits BEFORE multiband comp so sibilance doesn't pump the compressors
      if (dynamicDesserNode) {
        noiseHighShelf.connect(dynamicDesserNode);
        dynamicDesserNode.connect(deEsser); // deEsser acts as backup/static stage
      } else {
        noiseHighShelf.connect(deEsser);
      }

      // ── Multiband Compressor (4 bands) ───────────────────────────────────
      // Split into 4 parallel frequency bands, compress each independently, sum back
      deEsser.connect(mb_sub_lp);
      deEsser.connect(mb_lm_hp);
      deEsser.connect(mb_mid_hp);
      deEsser.connect(mb_air_hp);

      mb_sub_lp.connect(mb_sub_comp);    mb_sub_comp.connect(mb_sub_gain);    mb_sub_gain.connect(mbMerge);
      mb_lm_hp.connect(mb_lm_lp);       mb_lm_lp.connect(mb_lm_comp);        mb_lm_comp.connect(mb_lm_gain);  mb_lm_gain.connect(mbMerge);
      mb_mid_hp.connect(mb_mid_lp);      mb_mid_lp.connect(mb_mid_comp);      mb_mid_comp.connect(mb_mid_gain); mb_mid_gain.connect(mbMerge);
      mb_air_hp.connect(mb_air_comp);    mb_air_comp.connect(mb_air_gain);    mb_air_gain.connect(mbMerge);

      mbMerge.connect(compressor);

      // ── Broadband Compressor + Auto Gain ─────────────────────────────────
      compressor.connect(autoGain);
      autoGain.connect(eqWarmth);

      // ── EQ Colour / Warmth ────────────────────────────────────────────────
      eqWarmth.connect(eqClarity);

      // ── Spectral Dereverberation ──────────────────────────────────────────
      // Spectral dereverb worklet replaces old lowpass+duck approach when active
      // Old reverbFilter/Ducker remain in chain for legacy param compatibility
      if (dereverbNode) {
        eqClarity.connect(dereverbNode);
        dereverbNode.connect(reverbFilter);
      } else {
        eqClarity.connect(reverbFilter);
      }
      reverbFilter.connect(reverbDucker);
      reverbDucker.connect(duckGain);

      // ── Polish EQ + Brick-wall Limiter ───────────────────────────────────
      duckGain.connect(polishLow);
      polishLow.connect(polishPresence);
      polishPresence.connect(polishAir);
      polishAir.connect(limiter);
      limiter.connect(master);

      // ── LUFS Meter tap (meter-only, passes through) ──────────────────────
      if (lufsNode) {
        master.connect(lufsNode);
        lufsNode.connect(monitorGain);
        lufsNode.connect(destNode);
      } else {
        master.connect(monitorGain);
        master.connect(destNode);
      }

      // MONITORING OUTPUT DEVICE (AirPods / speakers)
      if (ctx.destination.setSinkId && selectedDevices.outputId !== 'default') {
        try {
          await ctx.destination.setSinkId(selectedDevices.outputId);
        } catch (err) {
          console.warn('setSinkId failed', err);
        }
      }
      monitorGain.connect(ctx.destination);

      processingRefs.current = {
        source,
        inputGain,
        lowCut,
        gateGain,
        gateOutput,
        noiseNotch,
        voiceContour,
        noiseBlend,
        noiseLowShelf,
        noiseHighShelf,
        deEsser,
        compressor,
        autoGain,
        eqWarmth,
        eqClarity,
        reverbFilter,
        reverbDucker,
        duckGain,
        polishLow,
        polishPresence,
        polishAir,
        limiter,
        master,
        monitorGain,
        analyser: ana,
        // new worklet nodes
        hyperGateNode,
        rnnoiseNode,
        rnnoiseDryGain,
        rnnoiseWetGain,
        lufsNode,
        lookaheadGateNode,
        dynamicDesserNode,
        dereverbNode,
        // multiband compressor nodes
        mb_sub_comp, mb_sub_gain,
        mb_lm_comp,  mb_lm_gain,
        mb_mid_comp, mb_mid_gain,
        mb_air_comp, mb_air_gain,
        mbMerge,
      };

      setAudioEngineError(null);
      setChainReady((prev) => prev + 1); // trigger DSP parameter useEffect
      visualizeAndGate();
    } catch (err) {
      console.error('Error starting audio engine', err);
      setAudioEngineError(
        err?.message ||
          'Could not start audio engine. Check microphone permissions and audio device.',
      );

      if (contextRef.current) {
        try {
          contextRef.current.close();
        } catch (closeError) {
          console.warn('Error closing audio context after failure', closeError);
        }
        contextRef.current = null;
      }
      setAudioContext(null);
    }
  };

  // The steps after the native engine is running: stats, routes, the analyser shim and the
  // visualiser. Used after a start and to adopt an engine that is already running (after a
  // reload, or when the app's flag fell out of step with the engine).
  const attachNativeEngine = (info) => {
    setAudioStats({
      sampleRate: info.sample_rate,
      bufferSize: info.buffer_frames || 256,
      latencyMs: info.latency_ms ?? null,
      callbackAvgMs: info.callback_avg_ms ?? null,
      callbackPeakMs: info.callback_peak_ms ?? null,
      cpuLoadPct: info.cpu_load_pct ?? null,
      droppedOutputSamples: info.dropped_output_samples ?? 0,
      state: 'running',
    });
    setNativeRouteStatus({
      input: info.input_device_name || info.device_name || null,
      monitor: info.monitor_output_name || null,
      broadcast: info.broadcast_output_name || null,
      monitor2: info.monitor2_output_name || null,
      monitor3: info.monitor3_output_name || null,
    });
    setNativeNoiseProfileCaptured(false);

    // Fake Web Audio analyser shim so visualizeAndGate() works unchanged.
    // Spectrum data is injected from 'audio-meters' Tauri events into tauriSpectrumRef.
    processingRefs.current.analyser = {
      frequencyBinCount: 128,
      // Native bins are normalised over -90..0 dB (Spectrum::take_bins).
      minDecibels: -90,
      maxDecibels: 0,
      getByteFrequencyData(arr) {
        const spec = tauriSpectrumRef.current;
        for (let i = 0; i < arr.length && i < spec.length; i++) {
          arr[i] = Math.round(spec[i] * 255);
        }
      },
      getByteTimeDomainData(arr) { arr.fill(128); },
    };

    setIsLive(true);
    setAudioEngineError(null);
    visualizeAndGate();
  };

  const toggleLive = async () => {
    if (isLive) {
      trackEvent('engine_stop');
      if (isTauri && mode === 'live') {
        await stopTauriEngine().catch(() => {});
        cleanupAudio({ stopNative: false });
      } else {
        cleanupAudio();
      }
      return;
    }

    // ── Tauri native Rust engine (CPAL — sub-5ms, direct hardware audio) ──────
    // In the desktop app "start" always means the native engine. From file mode it first returns
    // the rack to live; the browser microphone path does not work in the desktop webview.
    if (isTauri) {
      if (engineStarting) return;
      if (mode !== 'live') setMode('live');
      setEngineStarting(true);
      setAudioEngineError(null);
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        // The engine may already be running while this screen thinks it is off (a reload, or an
        // error after a start): adopt it rather than stop and start it, which drops the audio.
        const running = await getNativeEngineStatus(invoke).catch(() => null);
        if (running?.running) {
          attachNativeEngine(running);
          return;
        }
        cleanupAudio({ stopNative: false });
        const latestDevices = await getDevices();
        const resolvedDevices = reconcileSelectedDevices(selectedDevices, latestDevices);
        if (!devicesEqual(resolvedDevices, selectedDevices)) {
          setSelectedDevices(resolvedDevices);
        }

        const info = await startNativeEngine(invoke, resolvedDevices);
        attachNativeEngine(info);
        trackEvent('engine_start', {
          preset: 'tauri-rust',
          sr: info.sample_rate,
          monitor: info.monitor_output_name || 'default',
          broadcast: info.broadcast_output_name || 'not-set',
        });
      } catch (err) {
        console.error('Tauri engine start failed', err);
        setAudioEngineError(describeNativeEngineError(err));
      } finally {
        setEngineStarting(false);
      }
      return;
    }

    // ── Web Audio path (browser / Electron) ────────────────────────────────
    try {
      const constraints = {
        audio: {
          deviceId:
            selectedDevices.inputId !== 'default'
              ? { exact: selectedDevices.inputId }
              : undefined,
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: false,
        },
      };

      const s = await navigator.mediaDevices.getUserMedia(constraints);
      streamRef.current = s;

      if (audioElRef.current) {
        audioElRef.current.pause();
        setIsPlayingFile(false);
      }

      await startAudioEngine(s, null);

      // Only mark as live if engine actually started
      if (contextRef.current) {
        setIsLive(true);
        trackEvent('engine_start', { preset: speakerPreset });
      } else {
        setIsLive(false);
      }
    } catch (err) {
      console.error(err);
      alert(
        'Microphone access blocked or unavailable. Please check browser permissions and audio device.',
      );
      cleanupAudio();
    }
  };

  const handleModeSwitch = (newMode) => {
    // Choosing the mode you are already in changes nothing; it must not stop the engine.
    if (newMode === mode) return;
    // Leaving live for file stops the engine fully, so the header and the engine agree.
    if (isLive) {
      cleanupAudio();
      setIsLive(false);
    }
    setMode(newMode);

    if (newMode === 'file') {
      fileInputRef.current?.click();
    }
  };

  // ── Native menu IPC bridge (Tauri + legacy Electron) ────────────────────
  // Use a stable ref so the one-time listener always calls the latest handlers
  const menuHandlersRef = useRef(null);
  useEffect(() => {
    menuHandlersRef.current = {
      toggleLive, hardReset, startAutoCalibrate, startMicLearn,
      setMainTab, setIsBypassed, shareRecording, exportMp4,
      downloadWaveform, handleSnapshotSave, handleNoiseProfileCapture,
    };
  });
  useEffect(() => {
    const cleanup = onMenuCommand((event, ...args) => {
      const h = menuHandlersRef.current;
      if (!h) return;
      switch (event) {
        case 'menu:toggle-live':      h.toggleLive(); break;
        case 'menu:hard-reset':       h.hardReset(); break;
        case 'menu:show-settings':    setShowSettings(true); break;
        case 'menu:calibrate':        h.startAutoCalibrate(); break;
        case 'menu:learn-mic':        h.startMicLearn(); break;
        case 'menu:capture-noise':    h.handleNoiseProfileCapture(); break;
        case 'menu:set-tab':          h.setMainTab(args[0]); break;
        case 'menu:toggle-bypass':    h.setIsBypassed(b => !b); break;
        case 'menu:toggle-fullscreen': void toggleFullscreen(); break;
        case 'menu:zoom-in':          adjustPageZoom(1); break;
        case 'menu:zoom-out':         adjustPageZoom(-1); break;
        case 'menu:export-wav':       h.shareRecording('wav'); break;
        case 'menu:export-webm':      h.shareRecording('webm'); break;
        case 'menu:export-mp4':       h.exportMp4(); break;
        case 'menu:snapshot':         h.downloadWaveform(); break;
        case 'menu:save-snapshot':    h.handleSnapshotSave(); break;
        default: break;
      }
    });
    return cleanup;
  }, []);

  const startAutoCalibrate = () => {
    trackEvent('noise_calibrate');
    // Needs an active audio engine
    if (!processingRefs.current.analyser || !audioContext) {
      alert('Start Live or play a file first so I can listen to the room.');
      return;
    }

    calibrateRef.current = {
      active: true,
      startedAt: performance.now(),
      sumSq: 0,
      frames: 0,
    };
    setIsAutoCalibrating(true);
  };

  const [isMicLearning, setIsMicLearning] = useState(false);
  const [micLearned, setMicLearned] = useState(false);

  const startMicLearn = async () => {
    // Desktop: the native Hyper gate learns from the voice strip (5 s).
    if (isTauri && isLive) {
      setIsMicLearning(true);
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await learnNativeMicFingerprint(invoke);
        setMicLearned(true);
        setAudioEngineError(null);
      } catch (err) {
        setAudioEngineError(describeNativeEngineError(err, 'Could not learn the mic.'));
      } finally {
        setIsMicLearning(false);
      }
      return;
    }
    if (!processingRefs.current.analyser || !audioContext) {
      alert('Go Live first so I can learn your mic\'s spectral signature.');
      return;
    }
    const fp = micFingerprintRef.current;
    fp.learning = true;
    fp.learned = false;
    fp.frames = 0;
    fp.bandMeans = new Float32Array(8);
    fp.bandM2 = new Float32Array(8);
    fp.centroidMean = 0;
    fp.centroidM2 = 0;
    setIsMicLearning(true);
    setMicLearned(false);

    // Auto-complete after learning period (~5 seconds)
    setTimeout(() => {
      setIsMicLearning(false);
      if (micFingerprintRef.current.learned) {
        setMicLearned(true);
      }
    }, 5500);
  };

  // --- AUDIO + VIDEO FILE UPLOAD ---
  const handleFileUpload = (event) => {
    const file = event.target.files?.[0];
    if (!file) return;

    // Make sure each new file gets a fresh engine + clean state
    cleanupAudio();

    const isVideo = file.type.startsWith('video/');
    const url = URL.createObjectURL(file);

    setMode('file');
    setIsPlayingFile(true);
    setFileExportStatus(null);
    setFileInfo({
      name: file.name,
      type: file.type || (isVideo ? 'video/*' : 'audio/*'),
      isVideo,
    });

    if (isVideo) {
      const el = videoElRef.current;
      if (!el) return;
      el.src = url;
      el.currentTime = 0;
      el.onloadedmetadata = () => {
        const duration = el.duration || 0;
        setFileDuration(duration);
        setTrimStart(0);
        setTrimEnd(duration);
        setIsPreviewingTrim(false);

        startAudioEngine(null, el);
        const playPromise = el.play();
        if (playPromise && playPromise.catch) {
          playPromise.catch((err) => {
            console.error('Autoplay blocked or failed for video file', err);
            setIsPlayingFile(false);
          });
        }
      };
    } else {
      const el = audioElRef.current;
      if (!el) return;
      el.src = url;
      el.currentTime = 0;
      el.onloadedmetadata = () => {
        const duration = el.duration || 0;
        setFileDuration(duration);
        setTrimStart(0);
        setTrimEnd(duration);
        setIsPreviewingTrim(false);

        startAudioEngine(null, el);
        const playPromise = el.play();
        if (playPromise && playPromise.catch) {
          playPromise.catch((err) => {
            console.error('Autoplay blocked or failed for audio file', err);
            setIsPlayingFile(false);
          });
        }
      };
    }
  };

  // --- RECORD CHECK ---
  const toggleRecording = () => {
    if (recordingState === 'idle') {
      trackEvent('recording_start');
      if (!destNodeRef.current || !destNodeRef.current.stream) {
        alert('Start Live or load a file first so I have a processed stream to record.');
        return;
      }

      recordedChunksRef.current = [];
      const recorder = new MediaRecorder(destNodeRef.current.stream);
      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) recordedChunksRef.current.push(e.data);
      };
      recorder.onstop = () => {
        const blob = new Blob(recordedChunksRef.current, { type: 'audio/webm' });
        const url = URL.createObjectURL(blob);
        setRecordedUrl(url);
        setRecordingState('review');
        setIsMonitoring(false);
      };
      recorder.start();
      mediaRecorderRef.current = recorder;
      setRecordingState('recording');
      setRecordingDuration(0);
      recordingTimerRef.current = setInterval(() => {
        setRecordingDuration((prev) => prev + 1);
      }, 1000);
    } else if (recordingState === 'recording') {
      if (mediaRecorderRef.current) mediaRecorderRef.current.stop();
      clearInterval(recordingTimerRef.current);
    } else if (recordingState === 'review') {
      if (processingRefs.current.monitorGain) {
        processingRefs.current.monitorGain.gain.value = 0;
      }

      if (playbackAudioRef.current) {
        playbackAudioRef.current.pause();
        playbackAudioRef.current = null;
      }

      if (!recordedUrl) return;

      const testAudio = new Audio(recordedUrl);
      playbackAudioRef.current = testAudio;

      setPlaybackPosition(0);
      setPlaybackDuration(0);
      setIsPlayingBack(true);

      testAudio.addEventListener('loadedmetadata', () => {
        setPlaybackDuration(Math.floor(testAudio.duration || 0));
      });

      testAudio.addEventListener('timeupdate', () => {
        setPlaybackPosition(Math.floor(testAudio.currentTime || 0));
      });

      testAudio.addEventListener('ended', () => {
        setIsPlayingBack(false);
      });

      testAudio.play();
    }
  };

  const discardRecording = () => {
    setRecordedUrl(null);
    setRecordingState('idle');
    setRecordingDuration(0);
    setExportStatus(null);
    setPlaybackPosition(0);
    setPlaybackDuration(0);
    setIsPlayingBack(false);

    if (playbackAudioRef.current) {
      playbackAudioRef.current.pause();
      playbackAudioRef.current = null;
    }
  };

  const shareRecording = async (format = 'webm') => {
    if (!recordedUrl) return;
    setExportStatus('sharing');

    try {
      const timestamp = new Date().toLocaleTimeString().replace(/:/g, '-');

      if (format === 'wav') {
        // Convert WebM blob to WAV using AudioContext offline decoding
        const response = await fetch(recordedUrl);
        const webmBlob = await response.blob();
        const arrayBuffer = await webmBlob.arrayBuffer();
        const offlineCtx = new (window.OfflineAudioContext || window.webkitOfflineAudioContext)(1, 1, 48000);
        const audioBuffer = await offlineCtx.decodeAudioData(arrayBuffer);

        // Encode as WAV (PCM 16-bit)
        const wavBlob = audioBufferToWav(audioBuffer);
        const url = URL.createObjectURL(wavBlob);
        const a = document.createElement('a');
        a.style.display = 'none';
        a.href = url;
        a.download = `Tiwaton_Check_${timestamp}.wav`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
      } else {
        const a = document.createElement('a');
        a.style.display = 'none';
        a.href = recordedUrl;
        a.download = `Tiwaton_Check_${timestamp}.webm`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
      }

      setExportStatus('done');
    } catch (err) {
      console.error('Export error:', err);
      // Fallback to raw webm download
      const a = document.createElement('a');
      a.href = recordedUrl;
      a.download = `Tiwaton_Check_${Date.now()}.webm`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setExportStatus('done');
    }
  };

  // Convert AudioBuffer to WAV Blob (PCM 16-bit, mono/stereo)
  const audioBufferToWav = (buffer) => {
    const numChannels = buffer.numberOfChannels;
    const sampleRate = buffer.sampleRate;
    const format = 1; // PCM
    const bitDepth = 16;
    const bytesPerSample = bitDepth / 8;
    const blockAlign = numChannels * bytesPerSample;

    const samples = [];
    for (let ch = 0; ch < numChannels; ch++) {
      samples.push(buffer.getChannelData(ch));
    }

    const numFrames = buffer.length;
    const dataSize = numFrames * blockAlign;
    const headerSize = 44;
    const arrayBuffer = new ArrayBuffer(headerSize + dataSize);
    const view = new DataView(arrayBuffer);

    // WAV header
    const writeString = (offset, str) => {
      for (let i = 0; i < str.length; i++) view.setUint8(offset + i, str.charCodeAt(i));
    };
    writeString(0, 'RIFF');
    view.setUint32(4, 36 + dataSize, true);
    writeString(8, 'WAVE');
    writeString(12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, format, true);
    view.setUint16(22, numChannels, true);
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * blockAlign, true);
    view.setUint16(32, blockAlign, true);
    view.setUint16(34, bitDepth, true);
    writeString(36, 'data');
    view.setUint32(40, dataSize, true);

    // Interleave and write PCM samples
    let offset = 44;
    for (let i = 0; i < numFrames; i++) {
      for (let ch = 0; ch < numChannels; ch++) {
        const s = Math.max(-1, Math.min(1, samples[ch][i]));
        view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
        offset += 2;
      }
    }

    return new Blob([arrayBuffer], { type: 'audio/wav' });
  };

  // Download current waveform/spectrum visualization as PNG
  const downloadWaveform = () => {
    const canvas = canvasRef.current;
    if (!canvas) {
      alert('No visualization available. Go Live or load a file first.');
      return;
    }

    const timestamp = new Date().toLocaleTimeString().replace(/:/g, '-');
    canvas.toBlob((blob) => {
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.style.display = 'none';
      a.href = url;
      a.download = `Tiwaton_Spectrum_${timestamp}.png`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    }, 'image/png');
  };

  // --- MP4 EXPORT: Canvas waveform visualization + processed audio ---
  const exportMp4 = useCallback(async () => {
    const canvas = canvasRef.current;
    const dest = destNodeRef.current;
    if (!canvas || !dest || !dest.stream) {
      alert('Go Live first so TIWATON can capture the processed audio stream.');
      return;
    }
    trackEvent('export_mp4');

    // Combine canvas video track + processed audio track
    const canvasStream = canvas.captureStream(30); // 30fps
    const audioTracks = dest.stream.getAudioTracks();
    const combinedStream = new MediaStream([
      ...canvasStream.getVideoTracks(),
      ...audioTracks,
    ]);

    // Use the container we already negotiated for this browser, so the file
    // extension always matches what MediaRecorder actually produced.
    const { mimeType, extension } = videoFormat;

    const recorder = new MediaRecorder(combinedStream, { mimeType, videoBitsPerSecond: 2500000 });
    const chunks = [];

    recorder.ondataavailable = e => { if (e.data.size > 0) chunks.push(e.data); };
    recorder.onstop = () => {
      const blob = new Blob(chunks, { type: mimeType });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `TIWATON_session_${Date.now()}.${extension}`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      setExportStatus('done');
      setTimeout(() => setExportStatus(null), 3000);
    };

    recorder.start(100);
    setExportStatus('sharing');

    // Record for 10 seconds then auto-stop (user can also stop manually)
    setTimeout(() => {
      if (recorder.state === 'recording') recorder.stop();
    }, 10000);

    return recorder; // caller can stop early with recorder.stop()
  }, [videoFormat]);

  // --- FILE MODE: PROCESS & EXPORT (AI MASTER) ---
  const processAndExportFile = () => {
    if (!fileInfo) {
      alert('Load an audio or video file first.');
      return;
    }

    if (fileExportStatus === 'processing') {
      // Already running
      return;
    }

    if (!destNodeRef.current || !destNodeRef.current.stream) {
      alert('No processed audio stream found. Start playback or wait for file to load fully.');
      return;
    }

    const mediaEl =
      (fileInfo?.isVideo ? videoElRef.current : audioElRef.current) ||
      audioElRef.current ||
      videoElRef.current;

    if (!mediaEl || !mediaEl.src) {
      alert('Load an audio or video file first.');
      return;
    }

    // Compute effective trimmed region
    const fullDuration = mediaEl.duration || fileDuration || 0;
    const effectiveStart = Math.max(0, trimStart || 0);
    const effectiveEnd =
      Number.isFinite(trimEnd) && trimEnd > effectiveStart
        ? trimEnd
        : fullDuration || 0;

    if (!fullDuration || effectiveEnd <= effectiveStart) {
      alert('Trim selection is invalid. Check start/end and try again.');
      return;
    }

    try {
      mediaEl.pause();
      mediaEl.currentTime = effectiveStart;
    } catch (e) {
      console.warn('Could not reset media element.', e);
    }

    setFileExportStatus('processing');

    let recorder;
    const chunks = [];

    try {
      recorder = new MediaRecorder(destNodeRef.current.stream);
    } catch (err) {
      console.error(err);
      setFileExportStatus('error');
      alert('Browser does not support exporting processed audio.');
      return;
    }

    // Safety timeout in case "ended" never fires
    const effectiveDuration = effectiveEnd - effectiveStart;
    const maxRecordMs = (effectiveDuration > 0 ? effectiveDuration : 300) * 1000 + 3000;
    const timeoutId = setTimeout(() => {
      if (recorder && recorder.state === 'recording') {
        console.warn('Export timeout reached, stopping recorder.');
        recorder.stop();
      }
    }, maxRecordMs);

    recorder.ondataavailable = (e) => {
      if (e.data && e.data.size > 0) chunks.push(e.data);
    };

    recorder.onstop = async () => {
      clearTimeout(timeoutId);

      if (!chunks.length) {
        console.warn('No audio data captured during export.');
        setFileExportStatus('error');
        alert('No audio captured from this file. Please check your source and try again.');
        return;
      }

      const webmBlob = new Blob(chunks, { type: 'audio/webm' });

      const safeName = (fileInfo?.name || 'tiwaton_sermon')
        .replace(/\.[^/.]+$/, '')
        .slice(0, 60);

      // Convert to WAV for lossless export
      let downloadBlob = webmBlob;
      let ext = 'webm';
      try {
        const arrBuf = await webmBlob.arrayBuffer();
        const offCtx = new OfflineAudioContext(1, 1, 48000);
        const decoded = await offCtx.decodeAudioData(arrBuf);
        downloadBlob = audioBufferToWav(decoded);
        ext = 'wav';
      } catch (wavErr) {
        console.warn('WAV conversion failed, falling back to webm:', wavErr);
      }

      const url = URL.createObjectURL(downloadBlob);
      const a = document.createElement('a');
      a.style.display = 'none';
      a.href = url;
      a.download = `${safeName}_TIWATON_master.${ext}`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      setFileExportStatus('done');
    };

    const handleTimeUpdate = () => {
      // Stop when we reach the trim end (even if the underlying file is longer)
      if (mediaEl.currentTime >= effectiveEnd) {
        mediaEl.removeEventListener('timeupdate', handleTimeUpdate);
        mediaEl.pause();
        if (recorder.state === 'recording') {
          recorder.stop();
        }
      }
    };

    const handleEnded = () => {
      mediaEl.removeEventListener('ended', handleEnded);
      mediaEl.removeEventListener('timeupdate', handleTimeUpdate);
      if (recorder.state === 'recording') {
        recorder.stop();
      }
    };

    mediaEl.addEventListener('timeupdate', handleTimeUpdate);
    mediaEl.addEventListener('ended', handleEnded);

    const playPromise = mediaEl.play();
    if (playPromise && playPromise.catch) {
      playPromise
        .catch((err) => {
          console.error('Playback failed during export', err);
          mediaEl.removeEventListener('ended', handleEnded);
          mediaEl.removeEventListener('timeupdate', handleTimeUpdate);
          try {
            if (recorder.state === 'recording') {
              recorder.stop();
            }
          } catch (e) {
            console.warn('Error stopping recorder after playback failure', e);
          }
          setFileExportStatus('error');
          alert(
            'Unable to play this file for export. Try pressing play once manually, then export again.',
          );
        })
        .then(() => {
          if (recorder.state === 'inactive') {
            recorder.start();
          }
        });
    } else {
      recorder.start();
    }
  };

  const formatTime = (secs) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${s < 10 ? '0' : ''}${s}`;
  };

  const handleInputGainChange = (e) => {
    const val = parseFloat(e.target.value);
    setInputGainValue(val);
    if (processingRefs.current.inputGain && audioContext) {
      processingRefs.current.inputGain.gain.setTargetAtTime(
        val,
        audioContext.currentTime,
        0.1,
      );
    }
  };

  const handleTrimStartChange = (e) => {
    const v = parseFloat(e.target.value);
    if (!Number.isFinite(v) || fileDuration <= 0) return;

    // Ensure start is always before end
    const safe = Math.max(0, Math.min(v, trimEnd - 0.1));
    setTrimStart(safe);
  };

  const handleTrimEndChange = (e) => {
    const v = parseFloat(e.target.value);
    if (!Number.isFinite(v) || fileDuration <= 0) return;

    // Ensure end is always after start
    const safe = Math.min(fileDuration, Math.max(v, trimStart + 0.1));
    setTrimEnd(safe);
  };

  const clearTrim = () => {
    if (!fileDuration) return;
    setTrimStart(0);
    setTrimEnd(fileDuration);
  };

  const previewTrimSegment = () => {
    if (!fileInfo) {
      alert('Load a file first.');
      return;
    }

    const mediaEl =
      (fileInfo?.isVideo ? videoElRef.current : audioElRef.current) ||
      audioElRef.current ||
      videoElRef.current;

    if (!mediaEl || !mediaEl.src) {
      alert('No playable file found.');
      return;
    }

    if (!Number.isFinite(trimEnd) || trimEnd <= trimStart) {
      alert('Check your trim in/out points – end must be after start.');
      return;
    }

    try {
      mediaEl.pause();
      mediaEl.currentTime = trimStart || 0;
    } catch (e) {
      console.warn('Could not seek preview media element', e);
    }

    const handleTimeUpdate = () => {
      if (mediaEl.currentTime >= trimEnd) {
        mediaEl.removeEventListener('timeupdate', handleTimeUpdate);
        mediaEl.pause();
        setIsPreviewingTrim(false);
      }
    };

    mediaEl.addEventListener('timeupdate', handleTimeUpdate);
    setIsPreviewingTrim(true);

    const playPromise = mediaEl.play();
    if (playPromise && playPromise.catch) {
      playPromise.catch((err) => {
        console.error('Preview playback failed', err);
        setIsPreviewingTrim(false);
        mediaEl.removeEventListener('timeupdate', handleTimeUpdate);
      });
    }
  };

  // --- Parameter updates (EQ & compressor tweaks based on features) ---
  useEffect(() => {
    if (!audioContext || !processingRefs.current.compressor) {
      console.log('[TIWATON] DSP update skipped — chain not ready yet');
      return;
    }
    console.log('[TIWATON] DSP params applying — preset:', speakerPreset, 'features:', Object.entries(features).filter(([,v]) => v).map(([k]) => k).join(', '));
    const {
      compressor,
      eqWarmth,
      eqClarity,
      reverbFilter,
      reverbDucker,
      noiseNotch,
      voiceContour,
      noiseBlend,
      noiseLowShelf,
      noiseHighShelf,
      duckGain,
      polishLow,
      polishPresence,
      polishAir,
      limiter,
      master,
      deEsser,
      lookaheadGateNode,
      dynamicDesserNode,
      dereverbNode,
      mb_sub_comp,
      mb_lm_comp,
      mb_mid_comp,
      mb_air_comp,
    } = processingRefs.current;
    const now = audioContext.currentTime;

    const getTarget = (isActive, activeVal, passiveVal) =>
      isActive && !isBypassed ? activeVal : passiveVal;

    const mixActive = features.smartMixing || features.voicePattern;
    const masterActive = features.mastering || features.voicePattern;

    // Podcast-quality speaker presets — tuned for broadcast warmth, presence, and clarity
    const speakerPresets = {
      balanced: {
        warmth: 2,
        clarity: 2.5,
        deEsser: -1.5,
        bass: 1.5,
        air: 1.5,
        presence: 2,
      },
      pastor: {
        warmth: 5.5,
        clarity: 6,
        deEsser: -4,
        bass: 3,
        air: 3.5,
        presence: 4,
      },
      guest: {
        warmth: 3.5,
        clarity: 4,
        deEsser: -2.5,
        bass: 2,
        air: 2.5,
        presence: 3,
      },
      choir: {
        warmth: 2.5,
        clarity: 7,
        deEsser: -5,
        bass: 1,
        air: 4.5,
        presence: 5,
      },
    };
    const preset = speakerPresets[speakerPreset] || speakerPresets.balanced;
    const activeNoiseProfile = noiseProfiles.find(
      (profile) => profile.id === selectedNoiseProfileId,
    );

    // Dynamic speech boost: stronger when voice is active and pastor isolation is on
    const speechBoost = voiceActive ? (features.pastorIsolation ? 4 : 2) : 0;

    // --- Pastor Voice Isolation (Enhanced) ---
    // Close-mic speech has strong proximity effect: boosted 200-500Hz
    // Choir singing has energy spread across 500-4000Hz with harmonic peaks
    // Strategy: boost the "proximity zone" (200-500Hz) + presence (2-4kHz) for speech clarity
    //           while tightening the compressor to push speech forward dynamically
    if (eqClarity) {
      const baseClarity = preset.clarity + getTarget(features.pastorIsolation, 12, 0);
      // Tighter Q when pastor isolation = surgical presence boost, doesn't lift choir harmonics as much
      eqClarity.Q.value = getTarget(features.pastorIsolation, 1.8, 1);
      eqClarity.gain.linearRampToValueAtTime(baseClarity + speechBoost, now + 0.2);
    }

    if (eqWarmth) {
      // Pastor isolation: extra warmth in the proximity zone (close-mic advantage)
      const isoWarmth = features.pastorIsolation ? 4 : 0;
      const baseWarmth = preset.warmth + getTarget(features.sermonWarmth, 9, 0) + isoWarmth;
      // Narrower Q for pastor isolation = targets proximity zone precisely
      eqWarmth.Q.value = getTarget(features.pastorIsolation, 1.5, features.sermonWarmth ? 1.1 : 1);
      eqWarmth.gain.linearRampToValueAtTime(
        baseWarmth + speechBoost * 0.8,
        now + 0.25,
      );
    }

    if (deEsser) {
      // Pastor isolation: more aggressive de-essing to clean up the boosted presence
      const targetDeEsser = preset.deEsser + getTarget(features.pastorIsolation, -6, 0);
      deEsser.gain.linearRampToValueAtTime(targetDeEsser, now + 0.2);
    }

    // Voice contour: when pastor isolation is on, boost the speech intelligibility zone more aggressively
    if (voiceContour) {
      const baseContour = getTarget(features.voicePattern, 3.5, 0);
      const isoContour = features.pastorIsolation && !isBypassed ? 5 : 0;
      voiceContour.gain.linearRampToValueAtTime(baseContour + isoContour, now + 0.2);
      // Shift voice contour frequency: 1500Hz for speech intelligibility
      if (features.pastorIsolation) {
        voiceContour.frequency.linearRampToValueAtTime(1800, now + 0.2);
        voiceContour.Q.value = 1.2;
      } else {
        voiceContour.frequency.linearRampToValueAtTime(1500, now + 0.2);
        voiceContour.Q.value = 0.7;
      }
    }

    if (reverbFilter) {
      // When spectral dereverb worklet is active, keep legacy filter wide open
      // When only legacy mode: tighten to 6200 Hz to simulate reverb removal
      const useLegacyDereverb = features.dereverb && !dereverbNode;
      const targetFreq = getTarget(useLegacyDereverb, 6200, 18000);
      reverbFilter.frequency.linearRampToValueAtTime(targetFreq, now + 0.25);
    }

    if (reverbDucker) {
      const targetGain =
        features.dereverb && !isBypassed && !dereverbNode ? (voiceActive ? 1.0 : 0.55) : 1.0;
      reverbDucker.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

    // ── Spectral Dereverb Worklet ──────────────────────────────────────────
    if (dereverbNode) {
      dereverbNode.port.postMessage({
        enabled:  features.dereverb && !isBypassed,
        strength: features.pastorIsolation ? 2.0 : 1.5,
        floor:    0.08,
      });
    }

    // ── Lookahead Gate ─────────────────────────────────────────────────────
    if (lookaheadGateNode) {
      const enabledParam = lookaheadGateNode.parameters.get('enabled');
      if (enabledParam) enabledParam.value = (features.lookaheadGate && !isBypassed) ? 1 : 0;
    }

    // ── Dynamic De-esser ──────────────────────────────────────────────────
    if (dynamicDesserNode) {
      const enabledParam = dynamicDesserNode.parameters.get('enabled');
      if (enabledParam) enabledParam.value = (features.dynamicDeEsser && !isBypassed) ? 1 : 0;
      // Adjust de-esser threshold and frequency based on active features
      const freqParam    = dynamicDesserNode.parameters.get('frequency');
      const threshParam  = dynamicDesserNode.parameters.get('threshold');
      if (freqParam)   freqParam.value   = features.pastorIsolation ? 6500 : 7500;
      if (threshParam) threshParam.value = features.pastorIsolation ? -20  : -24;
    }

    // ── Multiband Compressor — enable/bypass per feature flag ────────────
    // When multibandComp is OFF, set all band gains to 0 dB so signal sums flat
    // (the deEsser → mbMerge path is always active; bypassed = each comp ratio=1)
    const mbActive = features.multibandComp && !isBypassed;
    if (mb_sub_comp)  mb_sub_comp.ratio.value  = mbActive ? 3   : 1;
    if (mb_lm_comp)   mb_lm_comp.ratio.value   = mbActive ? 3.5 : 1;
    if (mb_mid_comp)  mb_mid_comp.ratio.value  = mbActive ? 4   : 1;
    if (mb_air_comp)  mb_air_comp.ratio.value  = mbActive ? 2.5 : 1;
    // Tighten mid band further for pastor isolation (nail the voice band)
    if (mb_mid_comp && features.pastorIsolation) mb_mid_comp.ratio.value = mbActive ? 5.5 : 1;

    if (noiseNotch) {
      const targetQ = getTarget(features.voicePattern, 5, 1);
      noiseNotch.Q.value = targetQ;
    }

    // voiceContour is now handled in the pastor isolation section above

    if (noiseBlend) {
      const targetGain = getTarget(features.voicePattern, 0.92, 1.0);
      noiseBlend.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

    if (noiseLowShelf) {
      const targetGain = activeNoiseProfile ? activeNoiseProfile.lowReduction : 0;
      noiseLowShelf.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

    if (noiseHighShelf) {
      const targetGain = activeNoiseProfile ? activeNoiseProfile.highReduction : 0;
      noiseHighShelf.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

    if (duckGain) {
      const duckTarget =
        features.musicDucking && voiceActive && !isBypassed ? 0.72 : 1.0;
      duckGain.gain.setTargetAtTime(duckTarget, now, 0.12);
    }

    if (compressor) {
      // Pastor isolation: tighter compression to keep pastor voice above choir
      const pastorActive = features.pastorIsolation && voiceActive && !isBypassed;
      compressor.ratio.linearRampToValueAtTime(
        pastorActive ? 8 : getTarget(mixActive, 6.5, 1),
        now + 0.4,
      );
      compressor.threshold.linearRampToValueAtTime(
        pastorActive ? -35 : getTarget(mixActive, -30, 0),
        now + 0.4,
      );
      compressor.knee.linearRampToValueAtTime(
        pastorActive ? 12 : getTarget(mixActive, 20, 30),
        now + 0.4,
      );
      compressor.attack.linearRampToValueAtTime(
        pastorActive ? 0.001 : getTarget(mixActive, 0.002, 0.003),
        now + 0.35,
      );
      compressor.release.linearRampToValueAtTime(
        pastorActive ? 0.12 : getTarget(mixActive, 0.18, 0.25),
        now + 0.35,
      );
    }

    if (polishLow) {
      // Podcast bass body: warm low-end shelf for fullness (preset-aware)
      const bassBump = preset.bass || 0;
      const targetGain = getTarget(masterActive, 4.5 + bassBump, 0);
      polishLow.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

    if (polishPresence) {
      // Podcast presence: forward, "in your face" feel
      const presBump = preset.presence || 0;
      const targetGain = getTarget(masterActive, 5.5 + presBump, 0);
      polishPresence.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

    if (polishAir) {
      // Podcast air/sparkle: shimmer on top-end for clarity
      const airBump = preset.air || 0;
      const targetGain = getTarget(masterActive, 5 + airBump, 0);
      polishAir.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

    if (limiter) {
      // Podcast-grade brick-wall limiter: loud and clean, no clipping
      limiter.threshold.linearRampToValueAtTime(
        getTarget(masterActive, -3, 0),
        now + 0.2,
      );
      limiter.knee.linearRampToValueAtTime(
        getTarget(masterActive, 6, 30),
        now + 0.2,
      );
      limiter.ratio.linearRampToValueAtTime(
        getTarget(masterActive, 20, 1),
        now + 0.2,
      );
      limiter.attack.linearRampToValueAtTime(
        getTarget(masterActive, 0.001, 0.003),
        now + 0.2,
      );
      limiter.release.linearRampToValueAtTime(
        getTarget(masterActive, 0.08, 0.2),
        now + 0.2,
      );
    }

    if (master) {
      const loudnessGuard =
        features.streamingSafe && typeof loudnessDb === 'number'
          ? Math.max(0.7, 1 - Math.max(0, loudnessDb - loudnessTarget) * 0.05)
          : 1.0;
      const targetGain =
        (isBypassed ? 1.0 : 1.0 + speechBoost * 0.08) * loudnessGuard;
      master.gain.linearRampToValueAtTime(targetGain, now + 0.15);
    }
  }, [
    features,
    audioContext,
    chainReady,
    isBypassed,
    voiceActive,
    loudnessDb,
    loudnessTarget,
    speakerPreset,
    noiseProfiles,
    selectedNoiseProfileId,
  ]);

  // --- Visualizer + Gate + Smart Gain Rider ---
  const visualizeAndGate = () => {
    const { analyser, gateGain, noiseBlend, noiseNotch, noiseLowShelf, noiseHighShelf } = processingRefs.current;
    // The gate and rider logic below runs on every tab; drawing happens only while the live tab's
    // canvas exists, and follows it when the tab is left and shown again (a new canvas element).
    if (!analyser) return;
    if (animationRef.current) cancelAnimationFrame(animationRef.current);

    let drawCanvas = null;
    let ctx = null;
    const bufferLength = analyser.frequencyBinCount;
    const freqData = new Uint8Array(bufferLength);
    const timeData = new Uint8Array(bufferLength);

    let lastGateUpdate = 0;
    let lastVoiceUpdate = 0;
    let lastGainUpdate = 0;
    let lastPatternUpdate = 0;
    let lastNoiseUpdate = 0;
    let lastLoudnessUpdate = 0;
    let gateHoldUntil = 0;
    let voiceHoldUntil = 0;
    const updateInterval = 100; // ms
    const gainUpdateInterval = 200; // ms;
    const patternUpdateInterval = 120; // ms
    const noiseUpdateInterval = 140; // ms
    const loudnessUpdateInterval = 250; // ms

    const rootStyle = getComputedStyle(document.documentElement);
    const palette = readVisualiserPalette((name) => rootStyle.getPropertyValue(name));
    const scopeBands = bandEdges(audioContext?.sampleRate || 48000, bufferLength);
    const scopeRange = analyserRange(analyser);
    let scope = createScope(scopeBands.length);
    const waterfall = createWaterfall();
    let lastFrameAt = 0;

    const draw = (timestamp) => {
      animationRef.current = requestAnimationFrame(draw);

      analyser.getByteFrequencyData(freqData);
      analyser.getByteTimeDomainData(timeData);
      latestSpectrumRef.current = freqData.slice(0);

      const liveCanvas = canvasRef.current;
      if (liveCanvas !== drawCanvas) {
        drawCanvas = liveCanvas;
        ctx = liveCanvas ? liveCanvas.getContext('2d') : null;
      }
      const scopeSize = ctx ? fitCanvas(drawCanvas, ctx, window.devicePixelRatio || 1) : null;
      const frameDt = lastFrameAt ? Math.min(0.1, Math.max(0, (timestamp - lastFrameAt) / 1000)) : 0;
      lastFrameAt = timestamp;

      // Gate/voice state (declared early so spectrum bars can use them)
      let gateClosed = false;
      let localVoiceActive = false;
      let voiceCandidate = false;

      // Spectrum + band energies
      let lowEnergy = 0;
      let highEnergy = 0;
      const splitIndex = Math.floor(bufferLength * 0.4);

      for (let i = 0; i < bufferLength; i++) {
        const v = freqData[i];

        if (i < splitIndex) {
          lowEnergy += v;
        } else {
          highEnergy += v;
        }

      }

      const lowAvg = lowEnergy / (splitIndex || 1);
      const highAvg = highEnergy / (bufferLength - splitIndex || 1);
      const highBias = highAvg / (lowAvg + 1e-6);
      const antiChildNoiseActive = featuresRef.current.denoise;
      const highOnlyNoise = antiChildNoiseActive && highBias > 1.8 && lowAvg < 35;
      let bandMetricsReady = false;
      let voiceEnergy = 0;
      let lowNoise = 0;
      let highNoise = 0;
      let noiseEnergy = 0;
      let voiceRatio = 0;
      let spectralFlatness = 1;
      let zeroCrossingRate = 0;
      let formantScore = 0;
      let micMatchScore = 1.0;

      const ensureBandMetrics = () => {
        if (bandMetricsReady || !audioContext) return;
        const sampleRate = audioContext.sampleRate || 48000;
        const binHz = sampleRate / 2 / bufferLength;
        const bandAverage = (startHz, endHz) => {
          const startBin = Math.max(0, Math.floor(startHz / binHz));
          const endBin = Math.min(bufferLength - 1, Math.ceil(endHz / binHz));
          if (endBin <= startBin) return 0;
          let sum = 0;
          let count = 0;
          for (let i = startBin; i <= endBin; i += 1) {
            sum += freqData[i];
            count += 1;
          }
          return count ? sum / count : 0;
        };

        voiceEnergy = bandAverage(300, 3400);
        lowNoise = bandAverage(40, 180);
        highNoise = bandAverage(6000, 12000);
        noiseEnergy = (lowNoise + highNoise) / 2;
        voiceRatio = (voiceEnergy + 1) / (noiseEnergy + 1);

        // --- Spectral Flatness (Wiener entropy) ---
        // Flat spectrum (hissing/white noise) → value near 1.0
        // Peaked spectrum (speech/music) → value near 0.0
        // Rejects: fan noise, hissing, HVAC, static
        {
          const sfStart = Math.max(0, Math.floor(200 / binHz));
          const sfEnd = Math.min(bufferLength - 1, Math.ceil(5000 / binHz));
          let logSum = 0;
          let linSum = 0;
          let sfCount = 0;
          for (let i = sfStart; i <= sfEnd; i++) {
            const v = Math.max(freqData[i], 1); // floor to 1 to avoid log(0)
            logSum += Math.log(v);
            linSum += v;
            sfCount++;
          }
          if (sfCount > 0 && linSum > 0) {
            const geometricMean = Math.exp(logSum / sfCount);
            const arithmeticMean = linSum / sfCount;
            spectralFlatness = geometricMean / arithmeticMean; // 0..1
          }
        }

        // --- Zero-Crossing Rate (from time-domain) ---
        // Speech: moderate ZCR (0.05-0.25)
        // Hissing/noise: high ZCR (> 0.35)
        // Baby cry: very high ZCR or very low (tonal)
        {
          let crossings = 0;
          for (let i = 1; i < bufferLength; i++) {
            const prev = timeData[i - 1] - 128;
            const curr = timeData[i] - 128;
            if ((prev >= 0 && curr < 0) || (prev < 0 && curr >= 0)) {
              crossings++;
            }
          }
          zeroCrossingRate = crossings / bufferLength;
        }

        // --- Formant Structure Score ---
        // Human speech has 2-3 formant peaks (F1: 300-900Hz, F2: 900-2500Hz, F3: 2500-3500Hz)
        // Baby crying has narrow harmonic peaks, not formant structure
        // We check for energy peaks in at least 2 of 3 formant regions
        {
          const f1 = bandAverage(300, 900);
          const f2 = bandAverage(900, 2500);
          const f3 = bandAverage(2500, 3500);
          const valley1 = bandAverage(180, 300); // valley before F1
          const valley2 = bandAverage(3500, 5000); // valley after F3
          const avgValley = (valley1 + valley2) / 2 + 1;

          // Each formant should be notably stronger than the valleys
          const f1Peak = f1 / avgValley;
          const f2Peak = f2 / avgValley;
          const f3Peak = f3 / avgValley;

          // Score: how many formants are prominent (> 1.3x valley)
          let peaks = 0;
          if (f1Peak > 1.3) peaks++;
          if (f2Peak > 1.3) peaks++;
          if (f3Peak > 1.3) peaks++;

          // Spread check: baby cries concentrate in narrow band, speech spreads across F1-F3
          const maxFormant = Math.max(f1, f2, f3);
          const minFormant = Math.min(f1, f2, f3);
          const spread = maxFormant > 0 ? minFormant / maxFormant : 0;

          // Good speech: 2-3 peaks with reasonable spread (> 0.25)
          formantScore = peaks >= 2 && spread > 0.2 ? peaks + spread : 0;
        }

        // --- Mic Fingerprint: learn + score ---
        const fp = micFingerprintRef.current;
        const fpBands = [
          bandAverage(40, 180),   // sub/rumble
          bandAverage(180, 400),  // low-mid
          bandAverage(400, 900),  // mid (F1)
          bandAverage(900, 2000), // upper-mid (F2)
          bandAverage(2000, 3500),// presence
          bandAverage(3500, 5000),// brilliance
          bandAverage(5000, 8000),// high
          bandAverage(8000, 12000),// air/hiss
        ];

        // Spectral centroid (weighted average frequency)
        let centroidNum = 0, centroidDen = 0;
        for (let ci = 0; ci < bufferLength; ci++) {
          centroidNum += freqData[ci] * ci;
          centroidDen += freqData[ci];
        }
        const centroid = centroidDen > 0 ? centroidNum / centroidDen : 0;

        if (fp.learning && fp.frames < fp.learnFrames) {
          // Welford's online algorithm for mean + variance
          fp.frames++;
          for (let b = 0; b < 8; b++) {
            const delta = fpBands[b] - fp.bandMeans[b];
            fp.bandMeans[b] += delta / fp.frames;
            const delta2 = fpBands[b] - fp.bandMeans[b];
            fp.bandM2[b] += delta * delta2;
          }
          const cDelta = centroid - fp.centroidMean;
          fp.centroidMean += cDelta / fp.frames;
          fp.centroidM2 += cDelta * (centroid - fp.centroidMean);

          if (fp.frames >= fp.learnFrames) {
            fp.learned = true;
            fp.learning = false;
            console.log('[TIWATON] Mic fingerprint learned:', {
              bandMeans: Array.from(fp.bandMeans).map(v => v.toFixed(1)),
              centroidMean: fp.centroidMean.toFixed(1),
            });
          }
        }

        // Compute mic match score (0 = totally different, 1 = perfect match)
        micMatchScore = 1.0; // default: assume match if not learned yet
        if (fp.learned) {
          let totalDeviation = 0;
          for (let b = 0; b < 8; b++) {
            const variance = fp.bandM2[b] / (fp.frames - 1 || 1);
            const stdDev = Math.sqrt(variance) + 1; // +1 to avoid division by zero
            const deviation = Math.abs(fpBands[b] - fp.bandMeans[b]) / stdDev;
            totalDeviation += deviation;
          }
          // Centroid deviation
          const cVariance = fp.centroidM2 / (fp.frames - 1 || 1);
          const cStdDev = Math.sqrt(cVariance) + 0.1;
          const cDeviation = Math.abs(centroid - fp.centroidMean) / cStdDev;
          totalDeviation += cDeviation * 2; // centroid weighted double

          // Normalize: 10 checks, each ~1.0 if within 1 std dev
          // Score: 1.0 if avg deviation < 1, drops toward 0 as deviation increases
          const avgDev = totalDeviation / 10;
          micMatchScore = Math.max(0, Math.min(1, 1 - (avgDev - 1) * 0.3));
        }

        bandMetricsReady = true;
      };

      // Time-domain RMS
      let sumSq = 0;
      for (let i = 0; i < bufferLength; i++) {
        const sample = (timeData[i] - 128) / 128.0;
        sumSq += sample * sample;
      }
      const rms = Math.sqrt(sumSq / bufferLength);
      const db = 20 * Math.log10(rms || 0.00001);

      // Auto-calibrator
      if (calibrateRef.current.active) {
        calibrateRef.current.sumSq += sumSq;
        calibrateRef.current.frames += 1;

        const elapsed = timestamp - calibrateRef.current.startedAt;

        if (elapsed > 1800) {
          const meanSq =
            calibrateRef.current.sumSq / (calibrateRef.current.frames || 1);
          const roomRms = Math.sqrt(meanSq);
          const roomDb = 20 * Math.log10(roomRms || 0.00001);

          const targetThreshold = Math.max(-70, Math.min(roomDb + 8, -25));
          setNoiseFloorThreshold(targetThreshold);

          calibrateRef.current.active = false;
          setIsAutoCalibrating(false);
        }
      }

      // Gate decision + VAD
      // Skip main-thread gate if AudioWorklet is handling it (avoid double-gating)
      const workletHandlesGate = !!processingRefs.current.hyperGateNode;

      if (gateGain && settingsRef.current.denoise && !settingsRef.current.isBypassed && !workletHandlesGate) {
        const threshold = settingsRef.current.threshold;
        const openThreshold = threshold + 3;
        const closeThreshold = threshold - 4;
        const minGateGain = 0.015;

        ensureBandMetrics();

        // --- Tiered voice detection ---
        // Key principle: close-mic speech has STRONG low-mid energy (proximity effect)
        // TV/ambient from across room has flat/even distribution
        //
        // Tier 1 (LOUD + PROXIMITY): Very loud AND voice-band dominant = definitely speaking into mic
        const loudOverride = db > threshold + 15 && !highOnlyNoise && voiceRatio > 1.0;

        // Tier 2 (BASIC): Energy in voice band, above threshold, not pure high-freq noise
        const basicVoice =
          !highOnlyNoise &&
          lowAvg > 10 &&
          highBias < 3.0 &&
          db > threshold &&
          voiceEnergy > 10;

        // Tier 3 (SPECTRAL): Supplementary confidence checks
        const isNotNoise = spectralFlatness < 0.75;
        const isNotHissing = zeroCrossingRate < 0.35;
        const hasFormants = formantScore > 0;
        const hasVoiceEnergy = voiceEnergy > 15 && voiceRatio > 1.15;

        // Scoring system: accumulate confidence
        let voiceScore = 0;
        if (db > threshold) voiceScore += 1;
        if (db > openThreshold) voiceScore += 1;
        if (isNotNoise) voiceScore += 1;
        if (isNotHissing) voiceScore += 1;
        if (hasFormants) voiceScore += 2;
        if (hasVoiceEnergy) voiceScore += 2;
        if (voiceRatio > 1.0) voiceScore += 1;
        if (!highOnlyNoise) voiceScore += 1;
        // Mic fingerprint: if audio matches learned mic profile, boost confidence
        if (micMatchScore > 0.7) voiceScore += 2;
        else if (micMatchScore > 0.4) voiceScore += 1;
        // If audio clearly DOESN'T match mic profile (TV, ambient), penalize
        if (micMatchScore < 0.3) voiceScore -= 2;

        const speechOnlyMode = gateModeRef.current === 'speech';
        const scoreThreshold = speechOnlyMode ? 6 : 4;

        voiceCandidate = loudOverride || (basicVoice && voiceScore >= scoreThreshold);

        // Speech-only: require higher confidence
        if (speechOnlyMode && !loudOverride) {
          voiceCandidate = voiceCandidate && (hasVoiceEnergy || voiceScore >= 7);
        }

        if (voiceCandidate) {
          voiceHoldUntil = timestamp + 350;
        }
        localVoiceActive = voiceHoldUntil > timestamp;

        let target = minGateGain;

        // Can open for level: basic threshold check + not pure noise
        const canOpenForLevel =
          loudOverride ||
          (db > openThreshold && voiceScore >= (speechOnlyMode ? 5 : 3));

        if (canOpenForLevel || localVoiceActive) {
          target = 1.0;
          gateHoldUntil = timestamp + 250;
        } else if (timestamp < gateHoldUntil) {
          target = 1.0;
        } else if (db < closeThreshold) {
          target = minGateGain;
        } else {
          // Interpolation zone
          const progress =
            (db - closeThreshold) / (openThreshold - closeThreshold || 1);
          target = minGateGain + Math.max(0, Math.min(1, progress)) * (1 - minGateGain);
        }

        // Speech-only mode: reduce target when no voice detected (but don't block loud override)
        if (speechOnlyMode && !localVoiceActive && !loudOverride) {
          target = Math.min(target, 0.05);
        }

        // Lift for quiet speech that passed VAD
        if (localVoiceActive && db < threshold) {
          const lift = Math.min(0.5, 0.25 + (threshold - db) * 0.025);
          target = Math.max(target, lift);
        }

        const current = gateGain.gain.value;
        const smoothing = target > current ? 0.22 : 0.045;
        gateGain.gain.value = current + (target - current) * smoothing;

        gateClosed = gateGain.gain.value < 0.12;

        if (timestamp - lastGateUpdate > updateInterval) {
          setVisualizerGateStatus((prev) => (prev !== gateClosed ? gateClosed : prev));
          lastGateUpdate = timestamp;
        }
      } else if (workletHandlesGate && settingsRef.current.denoise && !settingsRef.current.isBypassed) {
        // AudioWorklet handles the actual gating, but we still need voice state for the visualizer
        const threshold = settingsRef.current.threshold;
        ensureBandMetrics();

        const loudOverrideVis = db > threshold + 15 && !highOnlyNoise && voiceRatio > 1.0;
        const basicVoiceVis = !highOnlyNoise && lowAvg > 10 && highBias < 3.0 && db > threshold && voiceEnergy > 10;
        let scoreVis = 0;
        if (db > threshold) scoreVis += 1;
        if (db > threshold + 3) scoreVis += 1;
        if (spectralFlatness < 0.75) scoreVis += 1;
        if (zeroCrossingRate < 0.35) scoreVis += 1;
        if (formantScore > 0) scoreVis += 2;
        if (voiceEnergy > 15 && voiceRatio > 1.15) scoreVis += 2;
        if (voiceRatio > 1.0) scoreVis += 1;
        if (!highOnlyNoise) scoreVis += 1;

        voiceCandidate = loudOverrideVis || (basicVoiceVis && scoreVis >= 4);
        if (voiceCandidate) voiceHoldUntil = timestamp + 350;
        localVoiceActive = voiceHoldUntil > timestamp;

        // gateClosed comes from worklet metrics (set via onmessage handler)
        gateClosed = visualizerGateStatus;
      } else if (gateGain) {
        // Gate off or bypassed: fully open
        gateGain.gain.value = 1.0;
        gateClosed = false;
        if (visualizerGateStatus !== false) {
          setVisualizerGateStatus(false);
        }
      }

      if (timestamp - lastVoiceUpdate > updateInterval) {
        const voiceNow = localVoiceActive && !highOnlyNoise;
        setVoiceActive((prev) => (prev !== voiceNow ? voiceNow : prev));
        lastVoiceUpdate = timestamp;
      }

      if (timestamp - lastLoudnessUpdate > loudnessUpdateInterval) {
        setLoudnessDb(parseFloat(db.toFixed(1)));
        lastLoudnessUpdate = timestamp;
      }

      if (
        timestamp - lastPatternUpdate > patternUpdateInterval &&
        featuresRef.current.voicePattern &&
        !isBypassed &&
        audioContext
      ) {
        lastPatternUpdate = timestamp;
        ensureBandMetrics();
        const ratio = (voiceEnergy + 1) / (noiseEnergy + 1);
        const blendTarget = Math.min(1, Math.max(0.35, ratio / 2));
        const notchTarget = lowNoise > highNoise ? 90 : 120;

        if (noiseBlend) {
          noiseBlend.gain.setTargetAtTime(
            blendTarget,
            audioContext.currentTime,
            0.15,
          );
        }

        if (noiseNotch) {
          noiseNotch.frequency.setTargetAtTime(
            notchTarget,
            audioContext.currentTime,
            0.2,
          );
        }
      }

      if (
        timestamp - lastNoiseUpdate > noiseUpdateInterval &&
        (settingsRef.current.denoise || featuresRef.current.voicePattern) &&
        !settingsRef.current.isBypassed &&
        audioContext
      ) {
        lastNoiseUpdate = timestamp;
        ensureBandMetrics();
        const noiseRatio = (lowNoise + highNoise + 1) / (voiceEnergy + 1);
        const activityBoost = localVoiceActive ? 1 : 0.6;
        const patternBoost = featuresRef.current.voicePattern ? 1.2 : 1.0;
        const dynamicLowCut = -Math.min(
          10,
          Math.max(2.0, noiseRatio * 2.8 * activityBoost * patternBoost),
        );
        const dynamicHighCut = -Math.min(
          12,
          Math.max(2.8, noiseRatio * 3.2 * activityBoost * patternBoost),
        );
        const activeProfile = noiseProfilesRef.current.find(
          (profile) => profile.id === selectedNoiseProfileIdRef.current,
        );
        const baseLow = activeProfile ? activeProfile.lowReduction : 0;
        const baseHigh = activeProfile ? activeProfile.highReduction : 0;

        if (noiseLowShelf) {
          noiseLowShelf.gain.setTargetAtTime(
            baseLow + dynamicLowCut,
            audioContext.currentTime,
            0.18,
          );
        }

        if (noiseHighShelf) {
          noiseHighShelf.gain.setTargetAtTime(
            baseHigh + dynamicHighCut,
            audioContext.currentTime,
            0.2,
          );
        }
      }

      // ===== LIVE ANALYSER (src/lib/liveScope.js) =====
      ensureBandMetrics();
      {
        const autoGainVal = processingRefs.current.autoGain?.gain?.value || 1;
        const state = gateState({
          denoise: settingsRef.current.denoise,
          bypassed: settingsRef.current.isBypassed,
          gateClosed,
          voiceActive: localVoiceActive,
        });
        const prevHistory = scope.history;
        scope = stepScope(scope, bandLevels(freqData, scopeBands), frameDt, {
          level: dbFraction(db, -70, 0),
          state,
        });
        if (ctx) drawScope(ctx, palette, scopeSize, {
          ...scope,
          state,
          confidence: Math.min(100, Math.max(0, (voiceRatio - 1) * 50)),
          riderDb: 20 * Math.log10(autoGainVal || 0.00001),
          thresholdDb: settingsRef.current.denoise && !settingsRef.current.isBypassed ? settingsRef.current.threshold : null,
          range: scopeRange,
        }, scopeBands, waterfall, scope.history !== prevHistory);
      }

      // Smart Gain Rider
      const autoGainNode = processingRefs.current.autoGain;
      const gainRiderEnabled =
        (featuresRef.current.smartMixing || featuresRef.current.voicePattern) &&
        !settingsRef.current.isBypassed;

      if (autoGainNode && gainRiderEnabled && localVoiceActive) {
        const aggressiveMode =
          settingsRef.current.denoise && settingsRef.current.threshold > -40;

        const targetSpeechDb = aggressiveMode ? -16 : -22;
        const maxBoostDb = aggressiveMode ? 20 : 16;
        const maxCutDb = -16;

        let neededChange = targetSpeechDb - db;
        if (neededChange > maxBoostDb) neededChange = maxBoostDb;
        if (neededChange < maxCutDb) neededChange = maxCutDb;

        const desiredLinear = Math.pow(10, neededChange / 20);
        const currentLinear = autoGainNode.gain.value || 1.0;

        const smoothingRider = aggressiveMode ? 0.03 : 0.02;
        autoGainNode.gain.value =
          currentLinear + (desiredLinear - currentLinear) * smoothingRider;

        if (timestamp - lastGainUpdate > gainUpdateInterval) {
          const appliedDb =
            20 * Math.log10(autoGainNode.gain.value || 0.00001);
          setGainRiderDb(parseFloat(appliedDb.toFixed(1)));
          lastGainUpdate = timestamp;
        }
      } else if (autoGainNode) {
        const currentLinear = autoGainNode.gain.value || 1.0;
        const targetLinear = 1.0;
        const smoothingBack = 0.05;

        autoGainNode.gain.value =
          currentLinear + (targetLinear - currentLinear) * smoothingBack;

        if (
          timestamp - lastGainUpdate > gainUpdateInterval &&
          gainRiderDbRef.current !== 0
        ) {
          setGainRiderDb(0);
          lastGainUpdate = timestamp;
        }
      }
    };

    draw(performance.now());
  };
//
  // Monitor gain on/off
  useEffect(() => {
    if (processingRefs.current.monitorGain && audioContext) {
      const shouldHear = isMonitoring && recordingState !== 'review';
      const gain = shouldHear ? 1 : 0;
      processingRefs.current.monitorGain.gain.setTargetAtTime(
        gain,
        audioContext.currentTime,
        0.1,
      );
    }
  }, [isMonitoring, recordingState, audioContext]);

  // ── Tauri: listen for audio-meters events → React state ────────────────────
  useEffect(() => {
    if (!isTauri || !isLive) return;
    let subscription = null;
    import('@tauri-apps/api/event').then(({ listen }) => {
      subscription = subscribeToNativeMeters(listen, (m) => {
        if (m.spectrum && m.spectrum.length > 0) {
          tauriSpectrumRef.current = new Float32Array(m.spectrum);
        }
        if (typeof m.lufs_m === 'number' && isFinite(m.lufs_m)) {
          setLufsM(parseFloat(m.lufs_m.toFixed(1)));
          setLufsST(parseFloat(m.lufs_st.toFixed(1)));
          setLufsI(parseFloat(m.lufs_i.toFixed(1)));
          setLoudnessDb(parseFloat(m.lufs_st.toFixed(1)));
        }
        if (typeof m.deess_gr_db === 'number') {
          setDesserGrDb(parseFloat(m.deess_gr_db.toFixed(1)));
        }
        if (typeof m.auto_gain_db === 'number') {
          setGainRiderDb(parseFloat(m.auto_gain_db.toFixed(1)));
        }
        if (typeof m.gate_gain === 'number') {
          setVisualizerGateStatus(m.gate_gain < 0.5);
        }
      });
      tauriUnlistenRef.current = () => {
        subscription?.dispose();
      };
    });
    return () => {
      subscription?.dispose();
      tauriUnlistenRef.current = null;
    };
  }, [isLive]);

  // ── Tauri: sync DSP params to Rust engine when settings change ─────────────
  useEffect(() => {
    if (!isTauri || !isLive) return;
    import('@tauri-apps/api/core').then(({ invoke }) => {
      syncNativeParams(invoke, {
        features,
        isBypassed,
        noiseFloorThreshold,
        inputGainValue,
        gateMode,
      }).catch(() => {});
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isLive, features.denoise, features.dereverb, features.dynamicDeEsser,
      isBypassed, noiseFloorThreshold, inputGainValue, gateMode]);

  // The status poll calls the latest cleanupAudio without re-subscribing on every render.
  const cleanupAudioRef = useRef(cleanupAudio);
  useEffect(() => {
    cleanupAudioRef.current = cleanupAudio;
  });

  useEffect(() => {
    if (!isTauri || !isLive) return;
    let cancelled = false;
    let intervalId = null;

    let misses = 0;
    import('@tauri-apps/api/core').then(({ invoke }) => {
      const poll = async () => {
        try {
          const status = await getNativeEngineStatus(invoke);
          if (cancelled) return;
          if (!status?.running) {
            // Two polls in a row without a running engine: it stopped on its own (a device was
            // unplugged, or it failed), so the screen must stop saying it is running.
            misses += 1;
            if (misses >= 2) cleanupAudioRef.current({ stopNative: false });
            return;
          }
          misses = 0;

          setAudioStats((prev) => ({
            ...prev,
            sampleRate: status.sample_rate ?? prev.sampleRate,
            bufferSize: status.buffer_frames ?? prev.bufferSize,
            latencyMs: status.latency_ms ?? prev.latencyMs,
            callbackAvgMs: status.callback_avg_ms ?? prev.callbackAvgMs,
            callbackPeakMs: status.callback_peak_ms ?? prev.callbackPeakMs,
            cpuLoadPct: status.cpu_load_pct ?? prev.cpuLoadPct,
            droppedOutputSamples: status.dropped_output_samples ?? prev.droppedOutputSamples,
            state: 'running',
          }));
          setNativeNoiseProfileCaptured(Boolean(status.noise_profile_ready));
          setComputerMutesSpeakers(computerAudioFromEngineStatus(status).mutesSpeakers);
          setNativeRouteStatus({
            input: status.input_device_name || status.device_name || null,
            monitor: status.monitor_output_name || null,
            broadcast: status.broadcast_output_name || null,
            monitor2: status.monitor2_output_name || null,
            monitor3: status.monitor3_output_name || null,
          });
        } catch {
          // Ignore transient polling failures while the engine is being restarted.
        }
      };

      void poll();
      intervalId = window.setInterval(() => {
        void poll();
      }, 1000);
    });

    return () => {
      cancelled = true;
      if (intervalId) {
        window.clearInterval(intervalId);
      }
    };
  }, [isLive]);

  // ── Tauri: the monitor key drives the native Monitor bus level ────────────────
  // The native engine scales the Monitor bus by monitor_gain_db; without this the key changed only
  // the browser path and the desktop monitor stayed on whatever the key showed.
  useEffect(() => {
    if (!isTauri) return;
    const on = isMonitoring && recordingState !== 'review';
    try { window.localStorage.setItem(MONITOR_PREF_KEY, isMonitoring ? 'on' : 'off'); } catch { /* storage may be blocked */ }
    if (!isLive) return;
    import('@tauri-apps/api/core')
      .then(({ invoke }) => setNativeMonitor(invoke, on))
      .catch(() => {});
  }, [isLive, isMonitoring, recordingState]);

  // ── Tauri: adopt an engine that is already running ──────────────────────────
  // After a reload the screen starts with the engine "off" while the Rust engine may still run.
  // Check on start-up and whenever the live tab is shown; only the tab change triggers this, so a
  // deliberate stop or reset is never undone.
  useEffect(() => {
    if (!isTauri || isLive || mode !== 'live' || mainTab !== 'live') return undefined;
    let cancelled = false;
    import('@tauri-apps/api/core')
      .then(({ invoke }) => getNativeEngineStatus(invoke))
      .then((status) => {
        if (!cancelled && status?.running) attachNativeEngine(status);
      })
      .catch(() => {});
    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mainTab]);

  // --- Helper: Resolve labels for Output Router panel ---//
  const monitorLabel = resolveDeviceLabel(availableDevices.outputs, selectedDevices.outputId);
  const broadcastLabel = resolveBroadcastLabel(selectedDevices, availableDevices.outputs);
  const broadcastRoute = describeBroadcastRoute(selectedDevices, availableDevices.outputs);
  const exportAvailability = getExportAvailability({
    isLive,
    recordingState,
    exportStatus,
    fileExportStatus,
    hasRecordedAudio: Boolean(recordedUrl),
    hasLoadedFile: Boolean(fileInfo),
  });

  const handleSnapshotSave = () => {
    trackEvent('snapshot_save');
    const nextId = snapshotCounterRef.current;
    snapshotCounterRef.current += 1;
    const label = `Snapshot ${nextId}`;
    setSnapshots((prev) => [
      ...prev,
      {
        id: nextId,
        label,
        features: { ...features },
        speakerPreset,
        selectedNoiseProfileId,
        noiseFloorThreshold,
        inputGainValue,
      },
    ]);
  };

  const handleSnapshotApply = (snapshot) => {
    setFeatures((prev) => ({ ...prev, ...snapshot.features }));
    setSpeakerPreset(snapshot.speakerPreset);
    // A noise profile cleared by reset can't be recalled; fall back to none.
    const profileId = snapshot.selectedNoiseProfileId || '';
    setSelectedNoiseProfileId(noiseProfiles.some((np) => np.id === profileId) ? profileId : '');
    if (snapshot.noiseFloorThreshold != null) setNoiseFloorThreshold(snapshot.noiseFloorThreshold);
    if (snapshot.inputGainValue != null) setInputGainValue(snapshot.inputGainValue);
  };

  const handleNoiseProfileCapture = async () => {
    if (isTauri && isLive) {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await captureNativeNoiseProfile(invoke);
        setNativeNoiseProfileCaptured(true);
        setAudioEngineError(null);
      } catch (err) {
        setAudioEngineError(describeNativeEngineError(err, 'Could not capture the native noise profile.'));
      }
      return;
    }

    const spectrum = latestSpectrumRef.current;
    if (!spectrum || spectrum.length === 0) return;
    const sampleRate = audioContext?.sampleRate || 48000;
    const binHz = sampleRate / 2 / spectrum.length;
    const bandAverage = (startHz, endHz) => {
      const startBin = Math.max(0, Math.floor(startHz / binHz));
      const endBin = Math.min(spectrum.length - 1, Math.ceil(endHz / binHz));
      if (endBin <= startBin) return 0;
      let sum = 0;
      let count = 0;
      for (let i = startBin; i <= endBin; i += 1) {
        sum += spectrum[i];
        count += 1;
      }
      return count ? sum / count : 0;
    };

    const lowEnergy = bandAverage(40, 200);
    const highEnergy = bandAverage(6000, 12000);
    const normalize = (val, min, max) =>
      Math.max(min, Math.min(max, val));
    const lowReduction = -normalize(lowEnergy / 18, 3, 12);
    const highReduction = -normalize(highEnergy / 16, 3, 14);
    const profileId = newNoiseProfileId();
    const label = `Noise Profile ${noiseProfileCounterRef.current}`;
    noiseProfileCounterRef.current += 1;

    setNoiseProfiles((prev) => [
      ...prev,
      {
        id: profileId,
        label,
        lowReduction,
        highReduction,
      },
    ]);
    setSelectedNoiseProfileId(profileId);
  };

  // --- RENDER ---
  return (
    <div className="fp-desk flex flex-col h-[100dvh] overflow-hidden select-none">
      <audio
        ref={audioElRef}
        crossOrigin="anonymous"
        onEnded={() => setIsPlayingFile(false)}
      />
      <video
        ref={videoElRef}
        className="hidden"
        crossOrigin="anonymous"
        onEnded={() => setIsPlayingFile(false)}
      />

      {/* Hidden file input */}
      <input
        type="file"
        ref={fileInputRef}
        accept="audio/*,video/*"
        className="hidden"
        onChange={handleFileUpload}
      />

      {/* ─── MENU BAR (browser only: the desktop app has the native menu) ─── */}
      {!isTauri && (
      <MenuBar
        onGoHome={goHome}
        onShowSettings={() => setShowSettings(true)}
        onShowObs={() => setShowObs(true)}
        onShowHelp={() => setShowHelp(true)}
        onToggleLive={toggleLive}
        onHardReset={hardReset}
        onStartAutoCalibrate={startAutoCalibrate}
        onStartMicLearn={startMicLearn}
        onCaptureNoise={handleNoiseProfileCapture}
        onHandleModeSwitch={handleModeSwitch}
        onSetMainTab={setMainTab}
        onSnapshotSave={handleSnapshotSave}
        onExportMp4={exportMp4}
        videoExportLabel={videoFormat.label}
        onShareRecording={shareRecording}
        onDownloadWaveform={downloadWaveform}
        onOpenLink={() => setShowLink(true)}
        isLive={isLive}
        mode={mode}
        mainTab={mainTab}
        recordingState={recordingState}
        isBypassed={isBypassed}
        setIsBypassed={setIsBypassed}
        features={features}
        setFeatures={setFeatures}
      />
      )}

      {/* ─── DESK HEADER (faceplate) ─── */}
      <DeskHeader
        isLive={isLive}
        isPlayingFile={isPlayingFile}
        audioStats={audioStats}
        gateClosed={visualizerGateStatus}
        onToggleEngine={toggleLive}
        onShowSettings={() => setShowSettings(true)}
        monitor={{
          on: isMonitoring && recordingState !== 'review',
          disabled: recordingState === 'review',
          onToggle: () => setIsMonitoring((m) => !m),
        }}
        bypassed={isBypassed}
        onToggleBypass={() => setIsBypassed((b) => !b)}
        onReset={hardReset}
        onShowObs={isTauri ? () => setShowObs(true) : null}
        onShowLink={() => setShowLink(true)}
        onShowHelp={() => setShowHelp(true)}
        loudness={{ st: lufsST, target: loudnessTarget }}
      />
      <HelpCorner open={showHelp} onClose={() => setShowHelp(false)} />

      <LinkPanel open={showLink} onClose={() => setShowLink(false)} />
      <ObsPanel open={showObs} onClose={() => setShowObs(false)} mixerController={mixerController} />

      <DeviceSettingsModal
        open={showSettings}
        onClose={() => setShowSettings(false)}
        onRefresh={getDevices}
        selectedDevices={selectedDevices}
        onSelectedDevicesChange={(patch) => {
          setSelectedDevices((prev) => ({ ...prev, ...patch }));
        }}
        availableDevices={availableDevices}
        audioStats={audioStats}
        mixerController={mixerController}
        isLive={isLive}
      />

      {/* ─── MAIN 3-PANEL WORKSPACE ─── */}
      <div className="flex-1 flex overflow-hidden min-h-0">

        <InputRack
          mode={mode}
          onModeSwitch={handleModeSwitch}
          core={{
            isLive,
            isPlayingFile,
            inputGain: inputGainValue,
            onInputGainChange: handleInputGainChange,
            deviceLabel: selectedDevices.inputId === 'default' ? 'default' : 'chosen in setup',
            features,
            setFeatures,
            gateClosed: visualizerGateStatus,
            gateMode,
            setGateMode,
            threshold: noiseFloorThreshold,
            setThreshold: setNoiseFloorThreshold,
            onCalibrate: startAutoCalibrate,
            calibrating: isAutoCalibrating,
            onLearnMic: startMicLearn,
            learning: isMicLearning,
            micLearned,
          }}
          voice={{ features, setFeatures, setGateMode, setSpeakerPreset, desserGrDb, gainRiderDb }}
          profiles={{
            roomName,
            setRoomName,
            isDesktop: isTauri,
            nativeCaptured: nativeNoiseProfileCaptured,
            canCapture: isTauri ? isLive : Boolean(latestSpectrumRef.current),
            onCapture: () => { void handleNoiseProfileCapture(); },
            selectedProfileId: selectedNoiseProfileId,
            setSelectedProfileId: setSelectedNoiseProfileId,
            profiles: noiseProfiles,
            speakerPreset,
            setSpeakerPreset,
            target: loudnessTarget,
            setTarget: setLoudnessTarget,
            lufsM,
            lufsST,
            lufsI,
            streamSafe: features.streamingSafe,
            onToggleStreamSafe: () => setFeatures((f) => ({ ...f, streamingSafe: !f.streamingSafe })),
          }}
          session={{
            showRouting: broadcastRoute.mode !== 'monitor-only',
            isLive,
            route: nativeRouteStatus,
            monitorLabel,
            broadcastLabel,
            stats: audioStats,
            abMode,
            setBypassed: setIsBypassed,
            snapshots,
            onSave: handleSnapshotSave,
            onApply: handleSnapshotApply,
          }}
        />

        {/* ══════════════════════════════════
            CENTER — MAIN WORKSPACE
        ══════════════════════════════════ */}
        <main className="fp-centre flex-1 flex flex-col min-w-0 overflow-hidden">

          {/* Centre bar: view keys and the record control */}
          <div className="fp-bar">
            <div className="fp-keys" role="tablist" aria-label="Views">
              {MAIN_TABS.map((tab) => (
                <Key key={tab.id} role="tab" aria-selected={mainTab === tab.id} lit={mainTab === tab.id ? 'sel' : null} onClick={() => setMainTab(tab.id)}>
                  {tab.label}
                </Key>
              ))}
            </div>
            {/* Desktop: the native multitrack recorder (strips + Stream mix). */}
            {isTauri && isLive && mainTab === 'live' && (
              <div className="fp-bar-end"><DesktopRecordButton /></div>
            )}
            {/* Browser: Web Audio record check of the processed stream. */}
            {!isTauri && isLive && mainTab === 'live' && (
              <div className="fp-bar-end">
                <RecordCheckKeys
                  state={recordingState}
                  duration={formatTime(recordingDuration)}
                  videoLabel={videoFormat.extension}
                  onToggle={toggleRecording}
                  onDiscard={discardRecording}
                  onShare={shareRecording}
                  onExportVideo={exportMp4}
                />
              </div>
            )}
          </div>

          {mainTab === 'live' && (
            <LiveView v={{
              isLive,
              isPlayingFile,
              mode,
              fileInfo,
              isBypassed,
              voiceActive,
              recordingState,
              isPlayingBack,
              playbackPosition,
              playbackDuration,
              formatTime,
              engineError: audioEngineError,
              engineStarting,
              onErrorBack: () => { cleanupAudio(); setIsLive(false); },
              onGoLive: toggleLive,
              loudness: { m: lufsM, st: lufsST, i: lufsI, target: loudnessTarget, limiterOn: Boolean(features.streamingSafe) },
              trim: {
                duration: fileDuration,
                start: trimStart,
                end: trimEnd,
                onStart: handleTrimStartChange,
                onEnd: handleTrimEndChange,
                onPreview: previewTrimSegment,
                previewing: isPreviewingTrim,
                onClear: clearTrim,
                onExport: processAndExportFile,
                canExport: Boolean(exportAvailability.canExportFileWav && destNodeRef.current && destNodeRef.current.stream),
                exportStatus: fileExportStatus,
              },
            }} canvasRef={canvasRef} />
          )}

          {/* ── EDITOR VIEW (Phase 2/3/4) ── */}
          {mainTab === 'editor' && (
            <div className="flex-1 overflow-hidden">
              <WaveformEditor audioContext={audioContext} />
            </div>
          )}

          {/* ── MIXER VIEW (Phase 1) ── */}
          {mainTab === 'mixer' && (
            <MixerConsole
              controller={mixerController}
              refreshKey={mixerRefreshKey(audioStats.state, nativeRouteStatus)}
              onOpenObs={() => setShowObs(true)}
              demoBanner={isTauri ? null : 'Demo mixer — live multichannel mixing runs in the desktop app'}
            />
          )}

        </main>{/* end center */}

        <OutputRouter r={{
          engineError: audioEngineError,
          broadcastSet: selectedDevices.broadcastBus !== 'Not set',
          droppedSamples: audioStats.droppedOutputSamples ?? 0,
          cpuPct: audioStats.cpuLoadPct,
          lufsST,
          target: loudnessTarget,
          computerMutesSpeakers: isLive && computerMutesSpeakers,
          onShowSettings: () => setShowSettings(true),
          isLive,
          isPlayingFile,
          monitoring: isMonitoring,
          reviewing: recordingState === 'review',
          onToggleMonitor: () => setIsMonitoring((m) => !m),
          monitorLabel,
          broadcastLabel,
          targetApp: outputTarget,
          onCycleTarget: cycleOutputTarget,
          features,
        }} />

      </div>{/* end main 3-panel */}

      {/* Small screens: the essentials in one bottom plate. */}
      <div className="lg:hidden">
      <nav className="fp-bar fixed bottom-0 left-0 right-0 z-40" aria-label="Quick controls" style={{ borderTop: '1px solid var(--fp-seam)', borderBottom: 0 }}>
        <Key lit={isLive ? 'red' : 'green'} onClick={mode === 'live' || isTauri ? toggleLive : undefined} disabled={mode !== 'live' && !isTauri}>
          {isLive ? 'stop' : 'start'}
        </Key>
        <Lamp tone={isLive ? 'g' : 'off'}>{isLive ? 'running' : 'standby'}</Lamp>
        <div className="fp-bar-end">
          <Key lit={isBypassed ? 'amber' : null} pressed={isBypassed} onClick={() => setIsBypassed(!isBypassed)}>bypass</Key>
          <Key onClick={() => setShowSettings(true)}>setup</Key>
        </div>
      </nav>
      </div>

    </div>
  );   // ✅ end of JSX returned from AudioProcessor
 }; // ✅ end of: const AudioProcessor = ({ goHome }) => {


export default TiwatonApp;
