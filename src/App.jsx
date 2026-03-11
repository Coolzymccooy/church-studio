import React, { useState, useEffect, useRef, useCallback } from 'react';
import WaveformEditor from './components/WaveformEditor';
import MenuBar from './components/MenuBar';
import AIAssistant from './components/AIAssistant';
import {
  Mic,
  Settings,
  Activity,
  Volume2,
  Sliders,
  Play,
  Square,
  Upload,
  Zap,
  Radio,
  CheckCircle2,
  AlertCircle,
  Cpu,
  Pause,
  Monitor,
  Video,
  X,
  Headphones,
  AlertTriangle,
  Power,
  Circle,
  Share2,
  Trash2,
  Check,
  ArrowRight,
  Waves,
  Speaker,
  RefreshCw,
  Menu,
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

const TiwatonApp = () => {
  const [view, setView] = useState(() => {
    if (typeof window === 'undefined') return 'landing';
    // Skip landing page in Electron desktop app — go straight to studio
    if (window.electronAPI?.isElectron) return 'studio';
    const savedView = window.localStorage.getItem('tiwaton:view');
    return savedView || 'landing';
  });

  useEffect(() => {
    if (typeof window === 'undefined') return;
    window.localStorage.setItem('tiwaton:view', view);
  }, [view]);

  if (view === 'landing') {
    return <LandingPage onEnter={() => setView('studio')} />;
  }

  return (
    <>
      <AudioProcessor goHome={window.electronAPI?.isElectron ? null : () => setView('landing')} />
      <HelpCorner />
    </>
  );
};

// --- LANDING PAGE ---
const LandingPage = ({ onEnter }) => {
  return (
    <div className="min-h-[100dvh] bg-slate-950 text-white relative overflow-hidden font-sans selection:bg-amber-500 selection:text-white">
      <div className="absolute inset-0 z-0 opacity-20 pointer-events-none">
        <div className="absolute top-[-20%] left-[-10%] w-[600px] h-[600px] bg-amber-600 rounded-full blur-[120px] animate-pulse" />
        <div
          className="absolute bottom-[-25%] right-[-15%] w-[520px] h-[520px] bg-purple-600 rounded-full blur-[140px]"
          style={{ animationDuration: '4s' }}
        />
      </div>

      <header className="relative z-10">
        <div className="mx-auto max-w-6xl px-6 pt-10 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 bg-gradient-to-tr from-amber-500 to-yellow-600 rounded-xl flex items-center justify-center shadow-[0_0_15px_rgba(245,158,11,0.6)]">
              <Waves size={22} className="text-white" />
            </div>
            <div className="text-sm uppercase tracking-[0.3em] text-slate-400">
              Tiwaton AI Studio
            </div>
          </div>
          <button
            onClick={onEnter}
            className="hidden md:inline-flex items-center gap-2 px-5 py-2 text-sm font-semibold bg-white text-slate-950 rounded-full hover:bg-amber-50 transition-colors"
          >
            Enter Studio <ArrowRight size={16} />
          </button>
        </div>
      </header>

      <main className="relative z-10">
        <section className="mx-auto max-w-6xl px-6 pt-14 pb-16 grid lg:grid-cols-[1.1fr_0.9fr] gap-12 items-center">
          <div className="text-left">
            <p className="text-sm uppercase tracking-[0.4em] text-amber-300 mb-4">
              Human-first audio automation
            </p>
            <h1 className="text-5xl md:text-7xl font-black tracking-tight leading-[1.05] mb-6 bg-clip-text text-transparent bg-gradient-to-r from-white via-amber-50 to-slate-400">
              The AI sound engineer that never misses a word.
            </h1>
            <p className="text-lg md:text-xl text-slate-300 leading-relaxed max-w-2xl mb-8">
              TIWATON turns chaotic church audio into a warm, consistent stream.
              Hyper-Gate keeps speech present, Echo/Reverb Cleaner controls room
              wash, and Smart Auto-Mixing delivers a broadcast-ready level every
              service.
            </p>
            <div className="flex flex-col sm:flex-row gap-4">
              <button
                onClick={onEnter}
                className="group relative px-8 py-4 bg-white text-slate-950 font-bold text-lg rounded-full overflow-hidden hover:scale-[1.02] transition-transform duration-300 shadow-[0_0_30px_rgba(255,255,255,0.25)]"
              >
                <div className="absolute inset-0 bg-gradient-to-r from-amber-400 to-yellow-500 opacity-0 group-hover:opacity-100 transition-opacity duration-300" />
                <span className="relative z-10 flex items-center gap-3 group-hover:text-white transition-colors">
                  Enter Studio <ArrowRight size={20} />
                </span>
              </button>
              <button className="px-8 py-4 rounded-full border border-slate-700 text-slate-200 hover:border-amber-400 hover:text-white transition-colors">
                Watch how it works
              </button>
            </div>
            <div className="mt-8 flex flex-wrap gap-6 text-sm text-slate-400">
              <span className="flex items-center gap-2">
                <CheckCircle2 size={16} className="text-emerald-400" />
                Built for live + file processing
              </span>
              <span className="flex items-center gap-2">
                <CheckCircle2 size={16} className="text-emerald-400" />
                OBS + vMix ready
              </span>
              <span className="flex items-center gap-2">
                <CheckCircle2 size={16} className="text-emerald-400" />
                AI-guided controls
              </span>
            </div>
          </div>

          <div className="bg-slate-900/60 border border-slate-800 rounded-3xl p-6 shadow-[0_20px_70px_rgba(15,23,42,0.6)]">
            <div className="flex items-center justify-between mb-6">
              <div>
                <p className="text-sm text-slate-400">Live service snapshot</p>
                <h2 className="text-2xl font-semibold">Audio clarity panel</h2>
              </div>
              <span className="px-3 py-1 text-xs rounded-full bg-emerald-500/20 text-emerald-200 border border-emerald-400/40">
                Live
              </span>
            </div>
            <div className="grid gap-4">
              {[
                {
                  icon: <Mic size={18} />,
                  title: 'Hyper-Gate',
                  description: 'Voice-first gating keeps speech in front.',
                },
                {
                  icon: <Waves size={18} />,
                  title: 'Echo / Reverb Cleaner',
                  description: 'Tames room wash and reflections instantly.',
                },
                {
                  icon: <Headphones size={18} />,
                  title: 'Pastor Voice Isolation',
                  description: 'Focuses on the lead mic and removes bleed.',
                },
                {
                  icon: <Speaker size={18} />,
                  title: 'Sermon Warmth',
                  description: 'Adds fullness without mud or rumble.',
                },
                {
                  icon: <Sliders size={18} />,
                  title: 'Smart Auto-Mixing',
                  description: 'Smooths levels for a broadcast-ready mix.',
                },
                {
                  icon: <Cpu size={18} />,
                  title: 'AI Mastering',
                  description: 'Consistent output every week, every device.',
                },
              ].map((item) => (
                <div
                  key={item.title}
                  className="flex items-start gap-4 p-4 rounded-2xl bg-slate-950/60 border border-slate-800/80"
                >
                  <div className="w-10 h-10 rounded-xl bg-amber-500/20 text-amber-200 flex items-center justify-center">
                    {item.icon}
                  </div>
                  <div>
                    <p className="text-base font-semibold text-white">
                      {item.title}
                    </p>
                    <p className="text-sm text-slate-400 leading-relaxed">
                      {item.description}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </section>

        <section className="mx-auto max-w-6xl px-6 pb-14 grid md:grid-cols-3 gap-6">
          {[
            {
              title: 'Human-designed flow',
              description:
                'Clear steps and visible feedback. Every control is labeled with a reason so volunteers feel confident.',
            },
            {
              title: 'AI that serves people',
              description:
                'Enhance the sermon, not the noise. TIWATON prioritizes speech intelligibility above everything.',
            },
            {
              title: 'Massive impact, minimal setup',
              description:
                'Connect a mic, select a mode, and go live. Your mix locks in within seconds.',
            },
          ].map((card) => (
            <div
              key={card.title}
              className="p-6 rounded-2xl bg-slate-900/70 border border-slate-800"
            >
              <h3 className="text-lg font-semibold text-white mb-2">
                {card.title}
              </h3>
              <p className="text-sm text-slate-400 leading-relaxed">
                {card.description}
              </p>
            </div>
          ))}
        </section>

        <section className="mx-auto max-w-6xl px-6 pb-16">
          <div className="rounded-3xl border border-slate-800 bg-gradient-to-r from-slate-900/80 via-slate-900/40 to-slate-900/80 p-10">
            <div className="grid lg:grid-cols-[1.2fr_0.8fr] gap-10 items-center">
              <div>
                <p className="text-sm uppercase tracking-[0.3em] text-amber-300 mb-3">
                  How it works
                </p>
                <h2 className="text-3xl md:text-4xl font-semibold text-white mb-4">
                  A clean, predictable path to broadcast-ready audio.
                </h2>
                <p className="text-base text-slate-300 leading-relaxed mb-6">
                  TIWATON follows a human-centered workflow: capture, enhance,
                  stabilize, and deliver. Every feature is stacked so speech
                  feels natural and the mix stays balanced even when volunteers
                  change week to week.
                </p>
                <div className="flex flex-wrap gap-6 text-sm text-slate-300">
                  <span className="flex items-center gap-2">
                    <Zap size={16} className="text-amber-300" />
                    Instant voice detection
                  </span>
                  <span className="flex items-center gap-2">
                    <Volume2 size={16} className="text-amber-300" />
                    Adaptive leveling
                  </span>
                  <span className="flex items-center gap-2">
                    <Radio size={16} className="text-amber-300" />
                    Stream-ready output
                  </span>
                </div>
              </div>
              <div className="space-y-4">
                {[
                  'Connect mic or mixer feed.',
                  'Enable Hyper-Gate + AI processors.',
                  'Watch voice clarity meters lock in.',
                  'Send to OBS, Zoom, or your stream encoder.',
                ].map((step, index) => (
                  <div
                    key={step}
                    className="flex items-center gap-4 p-4 rounded-2xl bg-slate-950/60 border border-slate-800"
                  >
                    <div className="w-10 h-10 rounded-full bg-amber-500/20 text-amber-200 flex items-center justify-center font-semibold">
                      {index + 1}
                    </div>
                    <p className="text-sm text-slate-300">{step}</p>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </section>

        <section className="mx-auto max-w-6xl px-6 pb-20">
          <div className="rounded-3xl bg-slate-900/70 border border-slate-800 p-10 flex flex-col lg:flex-row items-center justify-between gap-8">
            <div>
              <h3 className="text-3xl font-semibold text-white mb-2">
                Ready to hear the difference?
              </h3>
              <p className="text-slate-400 max-w-xl">
                Start a live session or upload your sermon. TIWATON will guide you
                through the right settings and deliver a polished mix.
              </p>
            </div>
            <button
              onClick={onEnter}
              className="px-8 py-4 rounded-full bg-amber-500 hover:bg-amber-400 text-white font-semibold shadow-[0_0_15px_rgba(245,158,11,0.4)]"
            >
              Launch Studio
            </button>
          </div>
        </section>
      </main>
    </div>
  );
};

// --- STUDIO COMPONENT ---
const AudioProcessor = ({ goHome }) => {
  const [isLive, setIsLive] = useState(false);
  const [isPlayingFile, setIsPlayingFile] = useState(false);
  const [audioContext, setAudioContext] = useState(null);
  const [mode, setMode] = useState('live');

  // OLD single "outputTarget" (display of which streaming app you're sending to)
  const [outputTarget, setOutputTarget] = useState('OBS Studio');

  const [showSettings, setShowSettings] = useState(false);
  const [showAI, setShowAI] = useState(false);
  const [isMonitoring, setIsMonitoring] = useState(false);
  const [isBypassed, setIsBypassed] = useState(false);
  const [exportStatus, setExportStatus] = useState(null);
  const [inputGainValue, setInputGainValue] = useState(1.0);
  const [isSidebarOpen, setIsSidebarOpen] = useState(false);
  const [voiceActive, setVoiceActive] = useState(false);
  const [isAutoCalibrating, setIsAutoCalibrating] = useState(false);
  const [audioEngineError, setAudioEngineError] = useState(null);
  const [chainReady, setChainReady] = useState(0); // increments when audio chain is fully built
  const [controlTab, setControlTab] = useState('core');
  const [mainTab, setMainTab] = useState('live'); // 'live' | 'editor'
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
  // - broadcastBus      → conceptual “broadcast output” (VB-Cable, Loopback, etc)
  const [selectedDevices, setSelectedDevices] = useState(() => {
    try {
      const saved = window.localStorage.getItem('tiwaton:devices');
      return saved ? JSON.parse(saved) : { inputId: 'default', outputId: 'default', broadcastBus: 'Not set' };
    } catch { return { inputId: 'default', outputId: 'default', broadcastBus: 'Not set' }; }
  });

  const [audioStats, setAudioStats] = useState({
    sampleRate: 0,
    bufferSize: 0,
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
      };
    } catch {
      return {
        denoise: false, dereverb: false, pastorIsolation: false,
        sermonWarmth: false, smartMixing: false, mastering: false,
        voicePattern: false, musicDucking: false, streamingSafe: false,
      };
    }
  });
  const [speakerPreset, setSpeakerPreset] = useState(() => {
    return window.localStorage.getItem('tiwaton:speakerPreset') || 'balanced';
  });
  const [loudnessDb, setLoudnessDb] = useState(null);
  const [loudnessTarget, setLoudnessTarget] = useState(-14);
  const [abMode, setAbMode] = useState('B');
  const [snapshots, setSnapshots] = useState([]);
  const snapshotCounterRef = useRef(1);
  const [noiseProfiles, setNoiseProfiles] = useState(() => {
    try {
      const saved = window.localStorage.getItem('tiwaton:noiseProfiles');
      return saved ? JSON.parse(saved) : [];
    } catch { return []; }
  });
  const [selectedNoiseProfileId, setSelectedNoiseProfileId] = useState(() => {
    return window.localStorage.getItem('tiwaton:selectedNoiseProfileId') || '';
  });
  const [roomName, setRoomName] = useState(() => {
    return window.localStorage.getItem('tiwaton:roomName') || 'Main Sanctuary';
  });
  const noiseProfileCounterRef = useRef(1);
  const latestSpectrumRef = useRef(null);
  const noiseFloorThresholdRef = useRef(noiseFloorThreshold);
  const featuresRef = useRef(features);
  const gateModeRef = useRef(gateMode);
  const noiseProfilesRef = useRef(noiseProfiles);
  const selectedNoiseProfileIdRef = useRef(selectedNoiseProfileId);
  const gainRiderDbRef = useRef(gainRiderDb);
  const canvasRef = useRef(null);
  const animationRef = useRef(null);

  useEffect(() => {
    isMonitoringRef.current = isMonitoring;
  }, [isMonitoring]);

  useEffect(() => {
    featuresRef.current = features;
    try { window.localStorage.setItem('tiwaton:features', JSON.stringify(features)); } catch {}
  }, [features]);

  useEffect(() => {
    gateModeRef.current = gateMode;
    try { window.localStorage.setItem('tiwaton:gateMode', gateMode); } catch {}
  }, [gateMode]);

  useEffect(() => {
    noiseProfilesRef.current = noiseProfiles;
    try { window.localStorage.setItem('tiwaton:noiseProfiles', JSON.stringify(noiseProfiles)); } catch {}
  }, [noiseProfiles]);

  useEffect(() => {
    selectedNoiseProfileIdRef.current = selectedNoiseProfileId;
    try { window.localStorage.setItem('tiwaton:selectedNoiseProfileId', selectedNoiseProfileId); } catch {}
  }, [selectedNoiseProfileId]);

  useEffect(() => {
    try { window.localStorage.setItem('tiwaton:roomName', roomName); } catch {}
  }, [roomName]);

  useEffect(() => {
    gainRiderDbRef.current = gainRiderDb;
  }, [gainRiderDb]);

  useEffect(() => {
    try { window.localStorage.setItem('tiwaton:devices', JSON.stringify(selectedDevices)); } catch {}
  }, [selectedDevices]);

  useEffect(() => {
    try { window.localStorage.setItem('tiwaton:speakerPreset', speakerPreset); } catch {}
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
    setSelectedDevices({
      inputId: 'default',
      outputId: 'default',
      broadcastBus: 'Not set',
    });
  }, []);

  const getDevices = useCallback(async () => {
    try {
      await navigator.mediaDevices.getUserMedia({ audio: true });
      const devices = await navigator.mediaDevices.enumerateDevices();
      setAvailableDevices({
        inputs: devices.filter((d) => d.kind === 'audioinput'),
        outputs: devices.filter((d) => d.kind === 'audiooutput'),
      });
    } catch (err) {
      console.warn('Permission needed', err);
      resetDeviceState();
    }
  }, [resetDeviceState]);

  useEffect(() => {
    if (showSettings) getDevices();
  }, [showSettings, getDevices]);

  useEffect(() => {
    noiseFloorThresholdRef.current = noiseFloorThreshold;
    try { window.localStorage.setItem('tiwaton:threshold', String(noiseFloorThreshold)); } catch {}
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
          bandStdDevs: Array.from(fp.bandM2).map((m2, i) =>
            fp.frames > 1 ? Math.sqrt(m2 / (fp.frames - 1)) : 0.5
          ),
        });
      }
    }

    // Sync RNNoise worklet enable/disable
    const rnNode = processingRefs.current.rnnoiseNode;
    if (rnNode) {
      rnNode.port.postMessage({
        type: 'config',
        enabled: features.voicePattern || features.denoise,
      });
    }

    // Force open gate if disabled
    if (!features.denoise && processingRefs.current.gateGain) {
      processingRefs.current.gateGain.gain.setValueAtTime(
        1.0,
        audioContext?.currentTime || 0
      );
    }
  }, [features.denoise, features.voicePattern, noiseFloorThreshold, gateMode, isBypassed, audioContext]);

  const cleanupAudio = useCallback(() => {
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
    setAudioStats({ sampleRate: 0, bufferSize: 0, state: 'suspended' });
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
    setAudioEngineError(null);
  }, []);

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
    });
    setSpeakerPreset('balanced');
    setLoudnessDb(null);
    setLoudnessTarget(-14);
    setAbMode('B');
    setSnapshots([]);
    snapshotCounterRef.current = 1;
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

  useEffect(() => {
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

      const ctx = new Ctor();

      if (ctx.state === 'suspended') {
        await ctx.resume();
      }

      contextRef.current = ctx;
      setAudioContext(ctx);
      setAudioStats({
        sampleRate: ctx.sampleRate,
        bufferSize: 128,
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

      try {
        await ctx.audioWorklet.addModule('/worklets/rnnoise-worklet.js');
        rnnoiseNode = new AudioWorkletNode(ctx, 'rnnoise-processor');

        // Try to load RNNoise WASM binary
        try {
          const wasmResponse = await fetch('/wasm/rnnoise.wasm');
          if (wasmResponse.ok) {
            const wasmBytes = await wasmResponse.arrayBuffer();
            rnnoiseNode.port.postMessage({ type: 'load-wasm', wasmBytes }, [wasmBytes]);
            console.log('[TIWATON] RNNoise WASM loaded (neural noise suppression active)');
          } else {
            console.warn('[TIWATON] RNNoise WASM not found at /wasm/rnnoise.wasm — neural denoising disabled. Place rnnoise.wasm in public/wasm/ to enable.');
          }
        } catch (wasmErr) {
          console.warn('[TIWATON] RNNoise WASM load failed:', wasmErr.message);
        }

        rnnoiseNode.port.onmessage = (event) => {
          if (event.data.type === 'vad') {
            // RNNoise VAD probability can reinforce our gate decisions
          }
        };
      } catch (err) {
        console.warn('[TIWATON] RNNoise worklet not available:', err.message);
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
      limiter.threshold.value = 0;
      limiter.knee.value = 30;
      limiter.ratio.value = 1;
      limiter.attack.value = 0.003;
      limiter.release.value = 0.2;

      const master = ctx.createGain();
      master.gain.value = 1.0;

      const ana = ctx.createAnalyser();
      ana.fftSize = 2048;

      const monitorGain = ctx.createGain();
      monitorGain.gain.value = isMonitoringRef.current ? 1.0 : 0.0;

      const destNode = ctx.createMediaStreamDestination();
      destNodeRef.current = destNode;

      // Chain connections
      source.connect(inputGain);
      inputGain.connect(lowCut);

      // Insert RNNoise (neural denoise) before analyser if available
      if (rnnoiseNode) {
        lowCut.connect(rnnoiseNode);
        rnnoiseNode.connect(ana);
      } else {
        lowCut.connect(ana);
      }

      // Insert HyperGate worklet after analyser if available, otherwise use GainNode fallback
      if (hyperGateNode) {
        ana.connect(hyperGateNode);
        hyperGateNode.connect(noiseNotch);
      } else {
        ana.connect(gateGain);
        gateGain.connect(noiseNotch);
      }
      noiseNotch.connect(voiceContour);
      voiceContour.connect(noiseBlend);
      noiseBlend.connect(noiseLowShelf);
      noiseLowShelf.connect(noiseHighShelf);
      noiseHighShelf.connect(deEsser);
      deEsser.connect(compressor);

      compressor.connect(autoGain);
      autoGain.connect(eqWarmth);

      eqWarmth.connect(eqClarity);
      eqClarity.connect(reverbFilter);
      reverbFilter.connect(reverbDucker);
      reverbDucker.connect(duckGain);
      duckGain.connect(polishLow);
      polishLow.connect(polishPresence);
      polishPresence.connect(polishAir);
      polishAir.connect(limiter);
      limiter.connect(master);

      // reverbDucker routes through duckGain→polish→limiter→master (no bypass)
      master.connect(monitorGain);
      master.connect(destNode);

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
        hyperGateNode,
        rnnoiseNode,
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

  const toggleLive = async () => {
    if (isLive) {
      // turning OFF live
      cleanupAudio();
      return;
    }

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
    if (isLive) cleanupAudio();
    setMode(newMode);

    if (newMode === 'file') {
      fileInputRef.current?.click();
    }
  };

  // ── Electron native menu IPC bridge ─────────────────────────────────────
  // Use a stable ref so the one-time IPC listener always calls the latest handlers
  const electronHandlersRef = useRef(null);
  useEffect(() => {
    electronHandlersRef.current = {
      toggleLive, hardReset, startAutoCalibrate, startMicLearn,
      setMainTab, setIsBypassed, shareRecording, exportMp4,
      downloadWaveform, handleSnapshotSave, handleNoiseProfileCapture,
    };
  });
  useEffect(() => {
    if (!window.electronAPI?.onMenuCommand) return;
    const cleanup = window.electronAPI.onMenuCommand((event, ...args) => {
      const h = electronHandlersRef.current;
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
        case 'menu:export-wav':       h.shareRecording('wav'); break;
        case 'menu:export-webm':      h.shareRecording('webm'); break;
        case 'menu:export-mp4':       h.exportMp4(); break;
        case 'menu:snapshot':         h.downloadWaveform(); break;
        case 'menu:save-snapshot':    h.handleSnapshotSave(); break;
        default: break;
      }
    });
    return cleanup;
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const startAutoCalibrate = () => {
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

  const startMicLearn = () => {
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

    // Combine canvas video track + processed audio track
    const canvasStream = canvas.captureStream(30); // 30fps
    const audioTracks = dest.stream.getAudioTracks();
    const combinedStream = new MediaStream([
      ...canvasStream.getVideoTracks(),
      ...audioTracks,
    ]);

    // Prefer WebM/VP8 (universal), fallback to whatever browser supports
    const mimeTypes = [
      'video/webm;codecs=vp8,opus',
      'video/webm;codecs=vp9,opus',
      'video/webm',
      'video/mp4',
    ];
    const mimeType = mimeTypes.find(t => MediaRecorder.isTypeSupported(t)) || 'video/webm';

    const recorder = new MediaRecorder(combinedStream, { mimeType, videoBitsPerSecond: 2500000 });
    const chunks = [];

    recorder.ondataavailable = e => { if (e.data.size > 0) chunks.push(e.data); };
    recorder.onstop = () => {
      const blob = new Blob(chunks, { type: mimeType });
      const ext = mimeType.includes('mp4') ? 'mp4' : 'webm';
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `TIWATON_session_${Date.now()}.${ext}`;
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
  }, []);

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
      const targetFreq = getTarget(features.dereverb, 6200, 18000);
      reverbFilter.frequency.linearRampToValueAtTime(targetFreq, now + 0.25);
    }

    if (reverbDucker) {
      const targetGain =
        features.dereverb && !isBypassed ? (voiceActive ? 1.0 : 0.55) : 1.0;
      reverbDucker.gain.linearRampToValueAtTime(targetGain, now + 0.2);
    }

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
    const { analyser, gateGain, noiseBlend, noiseNotch } = processingRefs.current;
    const canvas = canvasRef.current;
    if (!analyser || !canvas) return;
    if (animationRef.current) cancelAnimationFrame(animationRef.current);

    const ctx = canvas.getContext('2d');
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

    const draw = (timestamp) => {
      animationRef.current = requestAnimationFrame(draw);

      analyser.getByteFrequencyData(freqData);
      analyser.getByteTimeDomainData(timeData);
      latestSpectrumRef.current = freqData.slice(0);

      const w = canvas.width;
      const h = canvas.height;
      const cx = w / 2;
      const cy = h / 2;

      // Background gradient
      const bgGrad = ctx.createLinearGradient(0, 0, 0, h);
      bgGrad.addColorStop(0, '#020617');
      bgGrad.addColorStop(0.5, '#020617');
      bgGrad.addColorStop(1, '#020617');
      ctx.fillStyle = bgGrad;
      ctx.fillRect(0, 0, w, h);

      // Gate/voice state (declared early so spectrum bars can use them)
      let gateClosed = false;
      let localVoiceActive = false;
      let voiceCandidate = false;

      const barWidth = (w / bufferLength) * 2.5;
      let x = 0;

      // Spectrum + band energies
      let lowEnergy = 0;
      let highEnergy = 0;
      const splitIndex = Math.floor(bufferLength * 0.4);

      // Compute voice band bin range for coloring
      const sampleRateVal = audioContext?.sampleRate || 48000;
      const binHzVis = sampleRateVal / 2 / bufferLength;
      const voiceStartBin = Math.floor(300 / binHzVis);
      const voiceEndBin = Math.ceil(3400 / binHzVis);

      for (let i = 0; i < bufferLength; i++) {
        const v = freqData[i];
        const barHeight = v * 1.5;

        if (i < splitIndex) {
          lowEnergy += v;
        } else {
          highEnergy += v;
        }

        // Color-coded spectrum:
        // Green = voice band (300-3400 Hz) when voice is active
        // Cyan = voice band when gate is closed (potential voice)
        // Red = noise (high freq hiss / low freq rumble) when gate is rejecting
        // Orange = rejected noise in voice band (baby cry, etc)
        // Blue = neutral/inactive frequencies
        let barColor;
        const inVoiceBand = i >= voiceStartBin && i <= voiceEndBin;

        if (localVoiceActive && !gateClosed && inVoiceBand) {
          // Voice is active and gate is open - GREEN for confirmed voice
          barColor = 'rgba(34,197,94,0.85)'; // green-500
        } else if (localVoiceActive && !gateClosed) {
          // Voice active but this freq is outside voice band - dimmer green
          barColor = 'rgba(34,197,94,0.35)';
        } else if (gateClosed && inVoiceBand && v > 30) {
          // Gate closed but energy in voice band - ORANGE (rejected, could be baby/noise)
          barColor = 'rgba(251,146,60,0.7)'; // orange-400
        } else if (gateClosed && v > 20) {
          // Gate closed, noise being blocked - RED
          barColor = 'rgba(239,68,68,0.6)'; // red-500
        } else if (inVoiceBand) {
          // Voice band, quiet - CYAN
          barColor = 'rgba(56,189,248,0.75)'; // cyan-400
        } else {
          // Default - BLUE
          barColor = 'rgba(37,99,235,0.55)'; // blue-600
        }

        ctx.fillStyle = barColor;
        ctx.fillRect(x, h - barHeight / 2, barWidth, barHeight / 2);
        x += barWidth + 1;
      }

      const lowAvg = lowEnergy / (splitIndex || 1);
      const highAvg = highEnergy / (bufferLength - splitIndex || 1);
      const highBias = highAvg / (lowAvg + 1e-6);
      const antiChildNoiseActive = featuresRef.current.denoise;
      const highOnlyNoise = antiChildNoiseActive && highBias > 1.8 && lowAvg < 35;
      const harshHighNoise = highBias > 1.6 && lowAvg < 40;
      const stackActive =
        featuresRef.current.dereverb ||
        featuresRef.current.voicePattern ||
        featuresRef.current.musicDucking ||
        featuresRef.current.pastorIsolation ||
        featuresRef.current.sermonWarmth ||
        featuresRef.current.smartMixing ||
        featuresRef.current.mastering;
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

      // ===== CORE ORB =====
      const coreRadius = h * 0.35;
      const coreGrad = ctx.createRadialGradient(
        cx, cy, coreRadius * 0.1,
        cx, cy, coreRadius,
      );

      if (highOnlyNoise) {
        // Red pulse - noise detected, gate blocking
        coreGrad.addColorStop(0, 'rgba(248,113,113,0.55)');
        coreGrad.addColorStop(0.5, 'rgba(127,29,29,0.3)');
        coreGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
      } else if (!gateClosed && localVoiceActive) {
        // Green/cyan - confirmed voice passing through
        coreGrad.addColorStop(0, 'rgba(34,197,94,0.65)');
        coreGrad.addColorStop(0.35, 'rgba(56,189,248,0.4)');
        coreGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
      } else if (gateClosed) {
        // Dim red/purple - gate is closed, blocking noise
        coreGrad.addColorStop(0, 'rgba(168,85,247,0.3)');
        coreGrad.addColorStop(0.5, 'rgba(88,28,135,0.15)');
        coreGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
      } else {
        // Idle blue
        coreGrad.addColorStop(0, 'rgba(129,140,248,0.35)');
        coreGrad.addColorStop(0.5, 'rgba(30,64,175,0.25)');
        coreGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
      }

      ctx.fillStyle = coreGrad;
      ctx.beginPath();
      ctx.arc(cx, cy, coreRadius, 0, Math.PI * 2);
      ctx.fill();

      // ===== ON-CANVAS STATUS TEXT =====
      ctx.textAlign = 'center';

      // Gate status (large, center)
      if (settingsRef.current.denoise && !settingsRef.current.isBypassed) {
        const gateLabel = gateClosed ? 'GATE CLOSED' : 'GATE OPEN';
        const gateColor = gateClosed ? 'rgba(239,68,68,0.9)' : 'rgba(34,197,94,0.9)';
        ctx.font = 'bold 14px monospace';
        ctx.fillStyle = gateColor;
        ctx.fillText(gateLabel, cx, cy - 35);

        // Sub-label explaining what's happening
        ctx.font = '10px sans-serif';
        ctx.fillStyle = 'rgba(148,163,184,0.8)';
        if (gateClosed && highOnlyNoise) {
          ctx.fillText('Blocking hiss / high-freq noise', cx, cy - 20);
        } else if (gateClosed) {
          ctx.fillText('Waiting for voice from mic...', cx, cy - 20);
        } else if (localVoiceActive) {
          ctx.fillText('Voice detected - passing audio', cx, cy - 20);
        }
      }

      // Voice confidence / noise ratio display
      ensureBandMetrics();
      {
        const ratio = voiceRatio;
        const confidence = Math.min(100, Math.max(0, (ratio - 1) * 50));
        const barW = 120;
        const barH = 6;
        const barX = cx - barW / 2;
        const barY = cy + 5;

        // Background
        ctx.fillStyle = 'rgba(30,41,59,0.8)';
        ctx.beginPath();
        ctx.roundRect(barX - 2, barY - 2, barW + 4, barH + 4, 4);
        ctx.fill();

        // Fill - green when voice winning, red when noise winning
        const fillW = (confidence / 100) * barW;
        if (confidence > 50) {
          ctx.fillStyle = 'rgba(34,197,94,0.9)';
        } else if (confidence > 25) {
          ctx.fillStyle = 'rgba(250,204,21,0.9)';
        } else {
          ctx.fillStyle = 'rgba(239,68,68,0.8)';
        }
        ctx.beginPath();
        ctx.roundRect(barX, barY, Math.max(2, fillW), barH, 3);
        ctx.fill();

        // Label
        ctx.font = '9px sans-serif';
        ctx.fillStyle = 'rgba(148,163,184,0.7)';
        ctx.fillText(`Voice: ${Math.round(confidence)}%  /  Noise: ${Math.round(100 - confidence)}%`, cx, barY + barH + 14);
      }

      // Boost indicator (when gain rider is active)
      {
        const autoGainVal = processingRefs.current.autoGain?.gain?.value || 1;
        const boostDb = 20 * Math.log10(autoGainVal || 0.00001);
        if (Math.abs(boostDb) > 0.5) {
          const boostLabel = boostDb > 0
            ? `+${boostDb.toFixed(1)} dB BOOST`
            : `${boostDb.toFixed(1)} dB CUT`;
          ctx.font = 'bold 10px monospace';
          ctx.fillStyle = boostDb > 0 ? 'rgba(56,189,248,0.8)' : 'rgba(251,146,60,0.8)';
          ctx.fillText(boostLabel, cx, cy + 45);
        }
      }

      // Spectral quality indicators (top-left legend)
      {
        ctx.textAlign = 'left';
        ctx.font = '9px monospace';
        const legendX = 12;
        let legendY = 18;
        const lineH = 14;

        // Legend dots
        const dot = (color, label) => {
          ctx.fillStyle = color;
          ctx.beginPath();
          ctx.arc(legendX, legendY - 3, 4, 0, Math.PI * 2);
          ctx.fill();
          ctx.fillStyle = 'rgba(148,163,184,0.7)';
          ctx.fillText(label, legendX + 10, legendY);
          legendY += lineH;
        };

        dot('rgba(34,197,94,0.85)', 'Voice (passing)');
        dot('rgba(239,68,68,0.6)', 'Noise (blocked)');
        dot('rgba(251,146,60,0.7)', 'Rejected (baby/hiss)');
        dot('rgba(56,189,248,0.75)', 'Voice band (quiet)');

        ctx.textAlign = 'center';
      }

      // Red veil for harsh highs
      if (highOnlyNoise) {
        const noiseGrad = ctx.createLinearGradient(0, 0, 0, h * 0.4);
        noiseGrad.addColorStop(0, 'rgba(248,113,113,0.4)');
        noiseGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
        ctx.fillStyle = noiseGrad;
        ctx.fillRect(0, 0, w, h * 0.4);
      }

      // ===== BREATHING RING =====
      if (!gateClosed && localVoiceActive) {
        const t = (timestamp % 2000) / 2000;
        const pulse = 0.85 + 0.15 * Math.sin(t * Math.PI * 2);
        const ringRadius = coreRadius * pulse;

        ctx.beginPath();
        ctx.arc(cx, cy, ringRadius, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(34,197,94,0.6)';
        ctx.lineWidth = 3;
        ctx.setLineDash([10, 8]);
        ctx.stroke();
        ctx.setLineDash([]);
      } else if (gateClosed && settingsRef.current.denoise) {
        // Red dashed ring when gate is closed and blocking
        const ringRadius = coreRadius * 0.85;
        ctx.beginPath();
        ctx.arc(cx, cy, ringRadius, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(239,68,68,0.25)';
        ctx.lineWidth = 2;
        ctx.setLineDash([4, 8]);
        ctx.stroke();
        ctx.setLineDash([]);
      }

      // ===== THRESHOLD LINE =====
      if (settingsRef.current.denoise && !settingsRef.current.isBypassed) {
        const threshY =
          canvas.height * (1 - (settingsRef.current.threshold + 100) / 100);

        // Threshold line
        ctx.strokeStyle = '#6366f1';
        ctx.setLineDash([5, 5]);
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(0, threshY);
        ctx.lineTo(canvas.width, threshY);
        ctx.stroke();
        ctx.setLineDash([]);

        // Threshold label
        ctx.textAlign = 'right';
        ctx.font = '9px monospace';
        ctx.fillStyle = 'rgba(99,102,241,0.7)';
        ctx.fillText(`THRESHOLD ${settingsRef.current.threshold} dB`, w - 8, threshY - 4);
        ctx.textAlign = 'center';
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

  // --- Helper: Resolve labels for Output Router panel ---//
  const resolveOutputLabel = (id) => {
    if (id === 'default') return 'System Default';
    const found = availableDevices.outputs.find((d) => d.deviceId === id);
    if (found?.label) return found.label;
    return 'Custom Device';
  };

  const monitorLabel = resolveOutputLabel(selectedDevices.outputId);
  const broadcastLabel =
    selectedDevices.broadcastBus === 'Same as monitor'
      ? monitorLabel
      : selectedDevices.broadcastBus || 'Not set';

  const handleSnapshotSave = () => {
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
    setFeatures({ ...snapshot.features });
    setSpeakerPreset(snapshot.speakerPreset);
    setSelectedNoiseProfileId(snapshot.selectedNoiseProfileId || '');
    setNoiseFloorThreshold(snapshot.noiseFloorThreshold);
    setInputGainValue(snapshot.inputGainValue);
  };

  const handleNoiseProfileCapture = () => {
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
    const profileId = `profile-${noiseProfileCounterRef.current}`;
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
    <div className="flex flex-col h-[100dvh] text-white overflow-hidden select-none" style={{background:'#050A1C', fontFamily:'Inter, system-ui, sans-serif'}}>
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

      {/* ─── MENU BAR ─── */}
      <MenuBar
        onGoHome={goHome}
        onShowSettings={() => setShowSettings(true)}
        onToggleLive={toggleLive}
        onHardReset={hardReset}
        onStartAutoCalibrate={startAutoCalibrate}
        onStartMicLearn={startMicLearn}
        onCaptureNoise={handleNoiseProfileCapture}
        onHandleModeSwitch={handleModeSwitch}
        onSetMainTab={setMainTab}
        onSnapshotSave={handleSnapshotSave}
        onExportMp4={exportMp4}
        onShareRecording={shareRecording}
        onDownloadWaveform={downloadWaveform}
        isLive={isLive}
        mode={mode}
        mainTab={mainTab}
        recordingState={recordingState}
        isBypassed={isBypassed}
        setIsBypassed={setIsBypassed}
        features={features}
        setFeatures={setFeatures}
      />

      {/* ─── ENGINE STATUS BAR ─── */}
      <div className="h-9 shrink-0 flex items-center px-4 gap-4 text-[10px] z-20 border-b border-slate-800/40" style={{background:'#030710', fontFamily:'JetBrains Mono, monospace'}}>
        <span className={`flex items-center gap-1.5 font-bold ${isLive || isPlayingFile ? 'text-[#00E676]' : 'text-slate-600'}`}>
          <span className={`w-2 h-2 rounded-full ${isLive || isPlayingFile ? 'bg-[#00E676] animate-pulse' : 'bg-slate-700'}`}/>
          {isLive ? 'ENGINE ACTIVE' : isPlayingFile ? 'FILE PLAYBACK' : 'STANDBY'}
        </span>
        <div className="h-3 w-px bg-slate-800"/>
        <span className="text-slate-600">SR: <span className="text-slate-300">{audioStats.sampleRate > 0 ? (audioStats.sampleRate/1000).toFixed(0)+'kHz' : '--'}</span></span>
        <span className="text-slate-600">BUFFER: <span className="text-slate-300">{audioStats.bufferSize || '128'}</span></span>
        <span className="text-slate-600">LATENCY: <span className="text-slate-300">{audioStats.sampleRate > 0 ? ((audioStats.bufferSize || 128) / audioStats.sampleRate * 1000).toFixed(1)+'ms' : '--'}</span></span>
        <div className="h-3 w-px bg-slate-800"/>
        <span className={features.denoise && isLive ? 'text-[#F59E0B]' : 'text-slate-700'}>HYPERGATE:{features.denoise ? 'ON' : 'OFF'}</span>
        <span className={features.pastorIsolation && isLive ? 'text-[#F59E0B]' : 'text-slate-700'}>ISOLATION:{features.pastorIsolation ? 'ON' : 'OFF'}</span>
        <span className={features.mastering && isLive ? 'text-[#00E676]' : 'text-slate-700'}>POLISH:{features.mastering ? 'ON' : 'OFF'}</span>
        <div className="h-3 w-px bg-slate-800 ml-auto"/>
        <span className="text-slate-600 ml-0">VOICE: <span className={voiceActive ? 'text-[#00E676] font-bold' : 'text-slate-600'}>{voiceActive ? '92%' : '0%'}</span></span>
        <span className={`font-bold px-2 py-0.5 rounded text-[9px] ${visualizerGateStatus ? 'bg-[#FF5252]/15 text-[#FF5252]' : 'bg-[#00E676]/10 text-[#00E676]'}`}>
          {visualizerGateStatus ? 'GATE CLOSED' : 'GATE OPEN'}
        </span>
      </div>

      {showSettings && (
        <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center backdrop-blur-sm p-4">
          <div className="bg-slate-900 w-full max-w-lg rounded-2xl border border-slate-700 shadow-2xl p-6 space-y-6">
            <div className="flex justify-between items-center border-b border-slate-800 pb-4">
              <h2 className="text-xl font-bold flex items-center gap-2">
                <Settings size={20} className="text-amber-400" />
                Device Settings
              </h2>
              <div className="flex gap-2">
                <button
                  onClick={getDevices}
                  className="p-1.5 rounded-full bg-slate-800 text-slate-400 hover:text-white transition-colors"
                  title="Refresh Device List"
                >
                  <RefreshCw size={18} />
                </button>
                <button
                  onClick={() => setShowSettings(false)}
                  className="text-slate-400 hover:text-white"
                >
                  <X size={24} />
                </button>
              </div>
            </div>

            {/* INPUT SOURCE */}
            <div>
              <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
                Input Source (Mic / Mixer)
              </label>
              <select
                className="w-full bg-slate-950 border border-slate-700 rounded-lg p-3"
                value={selectedDevices.inputId}
                onChange={(e) =>
                  setSelectedDevices({
                    ...selectedDevices,
                    inputId: e.target.value,
                  })
                }
              >
                <option value="default">Default System Microphone</option>
                {availableDevices.inputs.map((d) => (
                  <option key={d.deviceId} value={d.deviceId}>
                    {d.label || `Mic ${d.deviceId.slice(0, 5)}...`}
                  </option>
                ))}
              </select>
            </div>

            {/* OUTPUT: MONITORING */}
            <div>
              <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
                Monitoring Output (what you hear)
              </label>
              <select
                className="w-full bg-slate-950 border border-slate-700 rounded-lg p-3"
                value={selectedDevices.outputId}
                onChange={(e) =>
                  setSelectedDevices({
                    ...selectedDevices,
                    outputId: e.target.value,
                  })
                }
              >
                <option value="default">Default System Output</option>
                {availableDevices.outputs.map((d) => (
                  <option key={d.deviceId} value={d.deviceId}>
                    {d.label || `Speaker ${d.deviceId.slice(0, 5)}...`}
                  </option>
                ))}
              </select>
              <p className="mt-1 text-[10px] text-slate-500">
                Choose AirPods, headphones, or speakers the sound engineer will monitor on.
              </p>
            </div>

            {/* OUTPUT: BROADCAST BUS (VIRTUAL CABLE) */}
            <div>
              <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
                Broadcast Output (what OBS / YouTube hears)
              </label>
              <select
                className="w-full bg-slate-950 border border-slate-700 rounded-lg p-3"
                value={selectedDevices.broadcastBus}
                onChange={(e) =>
                  setSelectedDevices({
                    ...selectedDevices,
                    broadcastBus: e.target.value,
                  })
                }
              >
                <option value="Not set">Not set yet (set device in OBS)</option>
                <option value="Same as monitor">Same as monitor output</option>
                {availableDevices.outputs.map((d) => (
                  <option key={d.deviceId} value={d.label || d.deviceId}>
                    {d.label || `Bus ${d.deviceId.slice(0, 5)}...`}
                  </option>
                ))}
              </select>
              <p className="mt-1 text-[10px] text-slate-500">
                Pick the virtual cable / loopback device you also select as the input in OBS
                (e.g. VB-CABLE, BlackHole, Loopback).
              </p>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div
                className={`p-4 rounded-xl border ${
                  audioStats.sampleRate > 0 && audioStats.sampleRate < 44100
                    ? 'bg-amber-900/20 border-amber-600/50'
                    : 'bg-slate-800 border-slate-700'
                }`}
              >
                <div className="text-xs font-bold mb-1">SAMPLE RATE</div>
                <div
                  className={`text-lg font-mono flex items-center gap-2 ${
                    audioStats.sampleRate > 0 && audioStats.sampleRate < 44100
                      ? 'text-amber-500'
                      : 'text-amber-400'
                  }`}
                >
                  {audioStats.sampleRate > 0
                    ? (audioStats.sampleRate / 1000).toFixed(1) + ' kHz'
                    : '--'}
                  {audioStats.sampleRate > 0 &&
                    audioStats.sampleRate < 44100 && (
                      <AlertTriangle size={16} />
                    )}
                </div>
              </div>
              <div className="bg-slate-800 p-4 rounded-xl border border-slate-700">
                <div className="text-xs font-bold mb-1">BUFFER SIZE</div>
                <div className="text-lg font-mono text-amber-400">
                  {audioStats.bufferSize || '--'} smp
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ─── MAIN 3-PANEL WORKSPACE ─── */}
      <div className="flex-1 flex overflow-hidden min-h-0">

        {/* ══════════════════════════════════
            LEFT — INPUT RACK
        ══════════════════════════════════ */}
        <div className="w-[272px] shrink-0 flex flex-col overflow-hidden border-r border-slate-800" style={{background:'#0D1428'}}>
          {/* Rack header */}
          <div className="px-3 py-2 border-b border-slate-800 flex items-center justify-between shrink-0">
            <span className="text-[9px] font-bold tracking-[0.2em] text-slate-500 uppercase">Input Rack</span>
            <div className="flex rounded p-0.5 border border-slate-800 gap-0.5" style={{background:'#050A1C'}}>
              <button onClick={() => handleModeSwitch('live')} className={`px-2 py-0.5 rounded text-[9px] font-bold transition-all ${mode === 'live' ? 'bg-[#F59E0B] text-white' : 'text-slate-500 hover:text-white'}`}>Live</button>
              <button onClick={() => handleModeSwitch('file')} className={`px-2 py-0.5 rounded text-[9px] font-bold transition-all ${mode === 'file' ? 'bg-[#F59E0B] text-white' : 'text-slate-500 hover:text-white'}`}>File</button>
            </div>
          </div>
          {/* Tab nav */}
          <div className="grid grid-cols-4 gap-0.5 p-2 border-b border-slate-800 shrink-0">
            {[{id:'core',label:'Core'},{id:'stack',label:'AI'},{id:'profiles',label:'Profiles'},{id:'session',label:'Session'}].map(tab => (
              <button key={tab.id} onClick={() => setControlTab(tab.id)}
                className={`py-1.5 rounded text-[9px] font-bold uppercase tracking-wide transition-colors ${controlTab === tab.id ? 'text-[#F59E0B] border border-[#F59E0B]/40' : 'text-slate-500 hover:text-slate-300 border border-transparent'}`}
                style={controlTab === tab.id ? {background:'rgba(245,158,11,0.12)'} : {}}>
                {tab.label}
              </button>
            ))}
          </div>
          {/* Tab content scrollable */}
          <div className="flex-1 overflow-y-auto p-3 space-y-3">

            {/* ── CORE TAB ── */}
            {controlTab === 'core' && (
              <div className="space-y-3">
                {/* Mic Input Module */}
                <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
                  <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{background:'rgba(255,255,255,0.03)'}}>
                    <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase flex items-center gap-1"><Mic size={9}/> Mic Input</span>
                    <span className={`w-2 h-2 rounded-full ${isLive || isPlayingFile ? 'bg-[#00E676] animate-pulse' : 'bg-slate-700'}`}/>
                  </div>
                  <div className="p-3 space-y-2">
                    <div className="flex justify-between items-center">
                      <span className="text-[10px] font-medium text-slate-300">Level</span>
                      <span className="text-[10px] text-[#F59E0B] font-mono">{(Math.log10(inputGainValue) * 20).toFixed(1)} dB</span>
                    </div>
                    <input type="range" min="0" max="4" step="0.1" value={inputGainValue} onChange={handleInputGainChange}
                      className="w-full h-1 rounded-lg appearance-none cursor-pointer" style={{background:'#F59E0B40'}} />
                    <div className="text-[9px] text-slate-600">Device: {selectedDevices.inputId === 'default' ? 'Default Mic' : 'Custom Input'}</div>
                  </div>
                </div>

                {/* Hyper Gate Module */}
                <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
                  <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{background:'rgba(255,255,255,0.03)'}}>
                    <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase flex items-center gap-1"><Activity size={9}/> Hyper Gate</span>
                    <span className={`text-[9px] font-bold px-1.5 py-0.5 rounded ${features.denoise && !visualizerGateStatus && isLive ? 'text-[#00E676]' : features.denoise && visualizerGateStatus ? 'text-[#FF5252]' : 'text-slate-700'}`}>
                      {features.denoise && isLive ? (visualizerGateStatus ? 'CLOSED' : 'OPEN') : 'OFF'}
                    </span>
                  </div>
                  <div className="p-3 space-y-2.5">
                    <div className="flex items-center gap-1.5">
                      <button onClick={() => setGateMode('balanced')} className={`flex-1 py-1 rounded text-[9px] font-bold border ${gateMode === 'balanced' ? 'border-[#F59E0B]/50 text-[#F59E0B]' : 'border-slate-700 text-slate-500 hover:text-slate-300'}`} style={gateMode === 'balanced' ? {background:'rgba(245,158,11,0.12)'} : {}}>Balanced</button>
                      <button onClick={() => setGateMode('speech')} className={`flex-1 py-1 rounded text-[9px] font-bold border ${gateMode === 'speech' ? 'border-amber-500/50 text-amber-300' : 'border-slate-700 text-slate-500 hover:text-slate-300'}`} style={gateMode === 'speech' ? {background:'rgba(245,158,11,0.12)'} : {}}>Speech Only</button>
                    </div>
                    <div onClick={() => setFeatures({...features, denoise: !features.denoise})} className={`cursor-pointer flex items-center justify-between p-2 rounded border transition-all ${features.denoise ? 'border-[#F59E0B]/40' : 'border-slate-800 hover:border-slate-700'}`} style={features.denoise ? {background:'rgba(245,158,11,0.08)'} : {}}>
                      <div className="flex items-center gap-2">
                        <Activity size={11} className={features.denoise ? 'text-[#F59E0B]' : 'text-slate-600'} />
                        <span className="text-[10px] font-semibold text-slate-300">Reduction</span>
                      </div>
                      <div className={`w-7 h-3.5 rounded-full transition-colors relative ${features.denoise ? 'bg-[#F59E0B]' : 'bg-slate-700'}`}>
                        <div className={`absolute top-0.5 w-2.5 h-2.5 bg-white rounded-full transition-transform ${features.denoise ? 'translate-x-3.5 left-0.5' : 'left-0.5'}`}/>
                      </div>
                    </div>
                    {features.denoise && (
                      <div className="space-y-2">
                        <div className="flex justify-between text-[9px]">
                          <span className="text-slate-500">Threshold</span>
                          <span className={`font-mono ${visualizerGateStatus ? 'text-[#FF5252]' : 'text-[#00E676]'}`}>{noiseFloorThreshold} dB</span>
                        </div>
                        <input type="range" min="-100" max="-20" step="1" value={noiseFloorThreshold} onChange={(e) => setNoiseFloorThreshold(parseFloat(e.target.value))}
                          className="w-full h-0.5 rounded-lg appearance-none cursor-pointer" style={{background:'rgba(245,158,11,0.4)'}} />
                        <div className="grid grid-cols-2 gap-1.5">
                          <button onClick={startAutoCalibrate} disabled={isAutoCalibrating} className={`py-1 rounded text-[9px] font-semibold border ${isAutoCalibrating ? 'border-[#F59E0B]/40 text-[#F59E0B] cursor-wait animate-pulse' : 'border-slate-700 text-slate-400 hover:border-slate-600'}`}>
                            {isAutoCalibrating ? 'Calibrating...' : 'Auto-Calibrate'}
                          </button>
                          <button onClick={startMicLearn} disabled={isMicLearning} className={`py-1 rounded text-[9px] font-semibold border ${isMicLearning ? 'border-cyan-500/40 text-cyan-300 cursor-wait animate-pulse' : micLearned ? 'border-emerald-500/40 text-emerald-300' : 'border-slate-700 text-slate-400 hover:border-slate-600'}`}>
                            {isMicLearning ? 'Learning...' : micLearned ? 'Re-Learn Mic' : 'Learn Mic'}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            )}

            {/* ── AI STACK TAB ── */}
            {controlTab === 'stack' && (
              <div className="space-y-2">
                <button onClick={() => { setFeatures({denoise:true,dereverb:true,voicePattern:true,musicDucking:true,pastorIsolation:true,sermonWarmth:true,smartMixing:true,mastering:true,streamingSafe:true}); setGateMode('speech'); setSpeakerPreset('pastor'); }}
                  className="w-full py-2 rounded-lg border border-[#00E676]/30 text-[#00E676] text-[9px] font-bold tracking-[0.15em] hover:opacity-90 transition-opacity" style={{background:'rgba(0,230,118,0.08)'}}>
                  ✦ CHURCH BETA — ALL ON
                </button>
                <p className="text-[9px] text-slate-600 text-center pb-1">All AI features + Speech-Only gate + Pastor preset</p>
                {[
                  {key:'dereverb', icon:<Radio size={11}/>, label:'Echo / Reverb Cleaner', desc:'Dries up room reflections'},
                  {key:'voicePattern', icon:<Waves size={11}/>, label:'Voice Pattern Cleanse', desc:'Removes non-voice spectral content'},
                  {key:'musicDucking', icon:<Volume2 size={11}/>, label:'Music Ducking', desc:'Drops music when speech detected'},
                  {key:'pastorIsolation', icon:<Mic size={11}/>, label:'Pastor Voice Isolation', desc:'Prioritises pastor over choir'},
                  {key:'sermonWarmth', icon:<Speaker size={11}/>, label:'Sermon Warmth', desc:'Low-mid body — podcast feel'},
                  {key:'smartMixing', icon:<Sliders size={11}/>, label:'Smart Auto-Mixing', desc:'Continuous level riding'},
                  {key:'mastering', icon:<Volume2 size={11}/>, label:'Voice Polish', desc:'Broadcast loudness + sheen'},
                ].map(({key, icon, label, desc}) => (
                  <div key={key} onClick={() => setFeatures({...features, [key]: !features[key]})}
                    className={`cursor-pointer p-2.5 rounded-lg border transition-all ${features[key] ? 'border-[#F59E0B]/40' : 'border-slate-800 hover:border-slate-700 opacity-60 hover:opacity-100'}`}
                    style={features[key] ? {background:'rgba(245,158,11,0.08)'} : {}}>
                    <div className="flex items-center justify-between">
                      <div className={`flex items-center gap-1.5 ${features[key] ? 'text-[#F59E0B]' : 'text-slate-500'}`}>
                        {icon}
                        <span className="text-[10px] font-semibold text-slate-300">{label}</span>
                      </div>
                      <div className={`w-6 h-3 rounded-full transition-colors relative ${features[key] ? 'bg-[#F59E0B]' : 'bg-slate-700'}`}>
                        <div className={`absolute top-0.5 w-2 h-2 bg-white rounded-full transition-transform ${features[key] ? 'translate-x-3 left-0.5' : 'left-0.5'}`}/>
                      </div>
                    </div>
                    <p className="text-[9px] text-slate-600 mt-1 pl-4">{desc}</p>
                  </div>
                ))}
              </div>
            )}

            {/* ── PROFILES TAB ── */}
            {controlTab === 'profiles' && (
              <div className="space-y-3">
                <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
                  <div className="px-3 py-1.5 border-b border-slate-800" style={{background:'rgba(255,255,255,0.03)'}}>
                    <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase">Room Profile</span>
                  </div>
                  <div className="p-3 space-y-2">
                    <input type="text" value={roomName} onChange={(e) => setRoomName(e.target.value)} placeholder="e.g. Main Sanctuary" className="w-full border border-slate-700 rounded p-1.5 text-[10px] text-slate-200 placeholder-slate-600" style={{background:'#050A1C'}} />
                    <div className="flex items-center justify-between">
                      <span className="text-[9px] text-slate-500">Noise Profile</span>
                      <button onClick={handleNoiseProfileCapture} disabled={!latestSpectrumRef.current} className={`px-2 py-0.5 rounded text-[9px] border ${latestSpectrumRef.current ? 'border-[#F59E0B]/40 text-[#F59E0B]' : 'border-slate-700 text-slate-600'}`} style={latestSpectrumRef.current ? {background:'rgba(245,158,11,0.1)'} : {}}>Capture</button>
                    </div>
                    <select className="w-full border border-slate-700 rounded p-1.5 text-[10px] text-slate-200" style={{background:'#050A1C'}} value={selectedNoiseProfileId} onChange={(e) => setSelectedNoiseProfileId(e.target.value)}>
                      <option value="">No profile</option>
                      {noiseProfiles.map(p => <option key={p.id} value={p.id}>{p.label}</option>)}
                    </select>
                  </div>
                </div>
                <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
                  <div className="px-3 py-1.5 border-b border-slate-800" style={{background:'rgba(255,255,255,0.03)'}}>
                    <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase">Speaker Preset</span>
                  </div>
                  <div className="p-2 space-y-1">
                    {[['balanced','Balanced'],['pastor','Pastor'],['guest','Guest Speaker'],['choir','Choir Lead']].map(([val, label]) => (
                      <button key={val} onClick={() => setSpeakerPreset(val)} className={`w-full py-1.5 px-2 rounded text-[10px] font-semibold border text-left transition-all ${speakerPreset === val ? 'border-[#F59E0B]/50 text-[#F59E0B]' : 'border-slate-800 text-slate-500 hover:text-slate-300'}`} style={speakerPreset === val ? {background:'rgba(245,158,11,0.12)'} : {}}>
                        {label}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
                  <div className="px-3 py-1.5 border-b border-slate-800" style={{background:'rgba(255,255,255,0.03)'}}>
                    <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase">Loudness Target</span>
                  </div>
                  <div className="p-3 space-y-2">
                    <select className="w-full border border-slate-700 rounded p-1.5 text-[10px] text-slate-200" style={{background:'#050A1C'}} value={loudnessTarget} onChange={(e) => setLoudnessTarget(Number(e.target.value))}>
                      <option value={-14}>YouTube Live (-14 LUFS)</option>
                      <option value={-16}>Facebook Live (-16 LUFS)</option>
                      <option value={-20}>Zoom / Teams (-20 LUFS)</option>
                    </select>
                    <div className="flex items-center justify-between text-[9px]">
                      <span className="text-slate-500">Current: {typeof loudnessDb === 'number' ? `${loudnessDb} dB` : '--'}</span>
                      <button onClick={() => setFeatures({...features, streamingSafe: !features.streamingSafe})} className={`px-2 py-0.5 rounded border text-[9px] font-bold ${features.streamingSafe ? 'border-[#00E676]/40 text-[#00E676]' : 'border-slate-700 text-slate-500'}`}>
                        STREAM SAFE {features.streamingSafe ? 'ON' : 'OFF'}
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* ── SESSION TAB ── */}
            {controlTab === 'session' && (
              <div className="space-y-3">
                <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
                  <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{background:'rgba(255,255,255,0.03)'}}>
                    <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase">A/B Compare</span>
                    <span className="text-[9px] text-[#F59E0B]">{abMode === 'A' ? 'A — Raw' : 'B — AI'}</span>
                  </div>
                  <div className="p-2 grid grid-cols-2 gap-1.5">
                    <button onClick={() => setIsBypassed(true)} className={`py-1.5 rounded border text-[10px] font-bold ${abMode === 'A' ? 'border-amber-500/50 text-amber-300' : 'border-slate-700 text-slate-600 hover:text-slate-400'}`} style={abMode === 'A' ? {background:'rgba(245,158,11,0.1)'} : {}}>A — Raw</button>
                    <button onClick={() => setIsBypassed(false)} className={`py-1.5 rounded border text-[10px] font-bold ${abMode === 'B' ? 'border-[#F59E0B]/50 text-[#F59E0B]' : 'border-slate-700 text-slate-600 hover:text-slate-400'}`} style={abMode === 'B' ? {background:'rgba(245,158,11,0.12)'} : {}}>B — AI</button>
                  </div>
                </div>
                <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
                  <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{background:'rgba(255,255,255,0.03)'}}>
                    <span className="text-[9px] font-bold tracking-[0.15em] text-slate-500 uppercase">Session Snapshots</span>
                    <button onClick={handleSnapshotSave} className="px-2 py-0.5 rounded border border-[#F59E0B]/40 text-[9px] text-[#F59E0B]" style={{background:'rgba(245,158,11,0.1)'}}>Save</button>
                  </div>
                  <div className="p-2 space-y-1">
                    {snapshots.length === 0
                      ? <p className="text-[9px] text-slate-600 p-1">Save a snapshot to recall your full mix.</p>
                      : snapshots.map(s => (
                        <button key={s.id} onClick={() => handleSnapshotApply(s)} className="w-full flex items-center justify-between px-2 py-1.5 rounded border border-slate-800 text-[10px] text-slate-300 hover:border-[#F59E0B]/30 hover:text-white">
                          <span>{s.label}</span>
                          <span className="text-[9px] text-slate-500">{s.speakerPreset}</span>
                        </button>
                      ))}
                  </div>
                </div>
              </div>
            )}

          </div>{/* end scrollable rack content */}
        </div>{/* end left rack */}

        {/* ══════════════════════════════════
            CENTER — MAIN WORKSPACE
        ══════════════════════════════════ */}
        <div className="flex-1 flex flex-col min-w-0 overflow-hidden" style={{background:'#050A1C'}}>

          {/* Center tab bar */}
          <div className="h-10 border-b border-slate-800 flex items-center px-3 gap-2 shrink-0" style={{background:'#0D1428'}}>
            <div className="flex rounded p-0.5 border border-slate-800 gap-0.5" style={{background:'#050A1C'}}>
              <button onClick={() => setMainTab('live')} className={`px-3 py-1 rounded text-[9px] font-bold tracking-wide transition-all ${mainTab === 'live' ? 'bg-[#F59E0B] text-white' : 'text-slate-500 hover:text-white'}`}>LIVE</button>
              <button onClick={() => setMainTab('editor')} className={`px-3 py-1 rounded text-[9px] font-bold tracking-wide transition-all ${mainTab === 'editor' ? 'bg-[#F59E0B] text-white' : 'text-slate-500 hover:text-white'}`}>EDITOR</button>
            </div>
            {/* Active feature badges */}
            <div className="flex items-center gap-1 overflow-x-auto" style={{scrollbarWidth:'none'}}>
              {features.denoise && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-[#00E676]/30 text-[#00E676] whitespace-nowrap" style={{background:'rgba(0,230,118,0.08)'}}>HYPER-GATE</span>}
              {features.voicePattern && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-[#00E676]/30 text-[#00E676] whitespace-nowrap" style={{background:'rgba(0,230,118,0.08)'}}>VOICE CLEANSE</span>}
              {features.pastorIsolation && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-[#F59E0B]/30 text-[#F59E0B] whitespace-nowrap" style={{background:'rgba(245,158,11,0.08)'}}>ISOLATION</span>}
              {features.sermonWarmth && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-[#F59E0B]/30 text-[#F59E0B] whitespace-nowrap" style={{background:'rgba(245,158,11,0.08)'}}>WARMTH</span>}
              {features.mastering && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-[#F59E0B]/30 text-[#F59E0B] whitespace-nowrap" style={{background:'rgba(245,158,11,0.08)'}}>POLISH</span>}
              {features.musicDucking && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-amber-500/30 text-amber-400 whitespace-nowrap" style={{background:'rgba(245,158,11,0.08)'}}>DUCKING</span>}
              {features.streamingSafe && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-amber-500/30 text-amber-400 whitespace-nowrap" style={{background:'rgba(245,158,11,0.08)'}}>STREAM SAFE</span>}
              {selectedNoiseProfileId && !isBypassed && <span className="px-1.5 py-0.5 rounded text-[8px] font-bold border border-slate-600 text-slate-400 whitespace-nowrap">NOISE PROFILE</span>}
            </div>
            {/* Recording check — right side of center bar */}
            {isLive && mainTab === 'live' && (
              <div className="ml-auto flex items-center">
                <div className={`flex items-center gap-1 p-0.5 pl-1.5 rounded-full border ${recordingState === 'recording' ? 'border-red-500 bg-red-900/60' : 'border-slate-600 bg-slate-800/60'}`}>
                  {recordingState === 'idle' && (
                    <button onClick={toggleRecording} className="flex items-center gap-1.5 px-2.5 py-1 bg-red-600 hover:bg-red-500 text-white rounded-full font-bold text-[9px]">
                      <Circle size={7} fill="currentColor"/> Record Check
                    </button>
                  )}
                  {recordingState === 'recording' && (
                    <button onClick={toggleRecording} className="flex items-center gap-2 px-2.5 py-1 text-white font-mono text-[9px]">
                      <span className="w-2 h-2 bg-red-500 rounded-sm animate-pulse"/> {formatTime(recordingDuration)} <span className="font-bold border-l border-red-500 pl-2 ml-1">STOP</span>
                    </button>
                  )}
                  {recordingState === 'review' && (
                    <div className="flex items-center text-[9px]">
                      <button onClick={toggleRecording} className="px-2 py-1 hover:bg-slate-700 rounded-l-full text-green-400 font-bold flex items-center gap-1"><Play size={9} fill="currentColor"/> PLAY</button>
                      <div className="w-px h-3 bg-slate-600"/>
                      <button onClick={discardRecording} className="px-2 py-1 hover:bg-slate-700 text-slate-400 hover:text-red-400"><Trash2 size={11}/></button>
                      <div className="w-px h-3 bg-slate-600"/>
                      <button onClick={() => shareRecording('webm')} className="px-2 py-1 hover:bg-slate-700 font-bold text-[#F59E0B] flex items-center gap-1"><Share2 size={9}/>WebM</button>
                      <div className="w-px h-3 bg-slate-600"/>
                      <button onClick={() => shareRecording('wav')} className="px-2 py-1 hover:bg-slate-700 font-bold text-amber-400 flex items-center gap-1"><Waves size={9}/>WAV</button>
                      <div className="w-px h-3 bg-slate-600"/>
                      <button onClick={downloadWaveform} className="px-2 py-1 hover:bg-slate-700 font-bold text-cyan-400 flex items-center gap-1"><Activity size={9}/>PNG</button>
                      <div className="w-px h-3 bg-slate-600"/>
                      <button onClick={exportMp4} className="px-2 py-1 hover:bg-slate-700 rounded-r-full font-bold text-purple-400 flex items-center gap-1"><Video size={9}/>MP4</button>
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>

          {/* ── LIVE VIEW ── */}
          {mainTab === 'live' && (
            <div className="flex-1 relative overflow-hidden">
              {/* Sound check overlay */}
              {!isLive && !isPlayingFile && mode === 'live' && (
                <div className="absolute inset-0 flex items-center justify-center backdrop-blur-sm z-30" style={{background:'rgba(0,0,0,0.65)'}}>
                  <div className="text-center p-8 rounded-2xl border border-slate-700 shadow-2xl w-full max-w-sm" style={{background:'#0D1428'}}>
                    <div className="w-16 h-16 rounded-full flex items-center justify-center mx-auto mb-4" style={{background:'rgba(245,158,11,0.15)', color:'#F59E0B'}}>
                      <Mic size={32}/>
                    </div>
                    <h3 className="text-xl font-bold mb-2">Sound Check</h3>
                    <p className="text-slate-400 text-sm mb-6">Connect mic or mixer, then go live.</p>
                    <button onClick={toggleLive} className="px-8 py-3 text-white font-bold rounded-lg shadow-lg flex items-center gap-2 mx-auto hover:opacity-90" style={{background:'#F59E0B'}}>
                      <Play size={18} fill="currentColor"/> Go Live
                    </button>
                  </div>
                </div>
              )}
              {/* File mode info */}
              {mode === 'file' && fileInfo && (
                <div className="absolute top-3 left-3 z-10 text-[9px] text-slate-300 px-2 py-1 rounded-full border border-slate-700" style={{background:'rgba(13,20,40,0.85)'}}>
                  File: <span className="text-white font-semibold">{fileInfo.name}</span>
                </div>
              )}
              {/* Bypass overlay */}
              {isBypassed && (
                <div className="absolute top-3 left-1/2 -translate-x-1/2 px-4 py-1 rounded-full text-[9px] font-bold tracking-widest shadow-lg animate-pulse z-20" style={{background:'#FF5252', color:'white'}}>
                  BYPASS MODE — RAW AUDIO
                </div>
              )}
              {/* Speech priority badge */}
              {voiceActive && !isBypassed && (
                <div className="absolute top-3 right-3 z-20">
                  <div className="px-3 py-1 rounded-full border flex items-center gap-1.5" style={{background:'rgba(13,20,40,0.85)', borderColor:'rgba(0,230,118,0.4)', boxShadow:'0 0 15px rgba(0,230,118,0.3)'}}>
                    <div className="w-1.5 h-1.5 rounded-full animate-pulse" style={{background:'#00E676'}}/>
                    <span className="text-[9px] font-bold tracking-wide uppercase" style={{color:'#00E676'}}>Speech Priority Active</span>
                  </div>
                </div>
              )}
              {/* Recording status */}
              {isLive && recordingState === 'recording' && (
                <div className="absolute bottom-14 left-1/2 -translate-x-1/2 z-20 text-[9px] text-red-400 font-bold tracking-widest animate-pulse">RECORDING TEST SIGNAL...</div>
              )}
              {isLive && recordingState === 'review' && (
                <div className="absolute bottom-14 left-1/2 -translate-x-1/2 z-20 flex flex-col items-center gap-1">
                  <span className="text-[9px] text-[#F59E0B] font-bold tracking-widest">LIVE MIC MUTED • REVIEWING</span>
                  {isPlayingBack && <span className="text-[9px] font-mono text-slate-400">{formatTime(playbackPosition)} / {playbackDuration ? formatTime(playbackDuration) : '--:--'}</span>}
                  {!isPlayingBack && playbackDuration && <span className="text-[9px] font-mono text-slate-400">Ready • {formatTime(playbackDuration)}</span>}
                </div>
              )}
              {/* Audio engine error */}
              {isLive && audioEngineError && (
                <div className="absolute inset-0 flex items-center justify-center backdrop-blur-sm z-30" style={{background:'rgba(0,0,0,0.7)'}}>
                  <div className="text-center p-6 rounded-2xl border border-red-500/70 shadow-2xl max-w-sm" style={{background:'#0D1428'}}>
                    <AlertTriangle className="w-8 h-8 text-red-400 mx-auto mb-3"/>
                    <h3 className="text-base font-bold mb-2">Audio engine failed</h3>
                    <p className="text-[10px] text-slate-300 mb-4 whitespace-pre-line">{audioEngineError}</p>
                    <button onClick={() => { cleanupAudio(); setIsLive(false); }} className="px-5 py-2 rounded-lg border border-slate-600 text-[10px] font-semibold" style={{background:'#050A1C'}}>Back to Sound Check</button>
                  </div>
                </div>
              )}
              {/* MAIN CANVAS */}
              <canvas ref={canvasRef} width={1000} height={500} className={`w-full h-full object-cover transition-opacity ${isBypassed ? 'opacity-40 grayscale' : 'opacity-90'}`}/>
              {/* File Trim editor */}
              {mode === 'file' && fileInfo && fileDuration > 0 && (
                <div className="absolute bottom-4 left-1/2 -translate-x-1/2 z-20 w-[min(760px,95%)]">
                  <div className="border border-slate-700 rounded-xl px-4 py-3 backdrop-blur" style={{background:'rgba(13,20,40,0.95)'}}>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-[9px] font-semibold text-slate-300 flex items-center gap-1.5"><Sliders className="w-3 h-3" style={{color:'#F59E0B'}}/> Trim Sermon Section</span>
                      <span className="text-[9px] font-mono text-slate-500">Total: {formatTime(Math.floor(fileDuration))}</span>
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <div className="flex justify-between text-[9px] text-slate-500 mb-1"><span>Start</span><span>{formatTime(Math.floor(trimStart || 0))}</span></div>
                        <input type="range" min="0" max={fileDuration || 0} step="0.1" value={trimStart} onChange={handleTrimStartChange} className="w-full h-0.5 rounded appearance-none cursor-pointer" style={{background:'rgba(245,158,11,0.3)'}}/>
                      </div>
                      <div>
                        <div className="flex justify-between text-[9px] text-slate-500 mb-1"><span>End</span><span>{formatTime(Math.floor(trimEnd || fileDuration || 0))}</span></div>
                        <input type="range" min="0" max={fileDuration || 0} step="0.1" value={trimEnd} onChange={handleTrimEndChange} className="w-full h-0.5 rounded appearance-none cursor-pointer" style={{background:'rgba(245,158,11,0.3)'}}/>
                      </div>
                    </div>
                    <div className="flex items-center gap-2 mt-2">
                      <button onClick={previewTrimSegment} disabled={isPreviewingTrim} className="px-3 py-1 rounded text-[9px] font-semibold border border-[#F59E0B]/40 text-[#F59E0B] hover:bg-[#F59E0B]/10 flex items-center gap-1">
                        {isPreviewingTrim ? <span className="w-2 h-2 rounded-full border border-[#F59E0B] border-t-transparent animate-spin"/> : <Play className="w-2.5 h-2.5"/>}
                        {isPreviewingTrim ? 'Previewing...' : 'Preview'}
                      </button>
                      <button onClick={clearTrim} className="px-3 py-1 rounded text-[9px] font-semibold border border-slate-700 text-slate-400 hover:bg-slate-800">Full File</button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* ── EDITOR VIEW (Phase 2/3/4) ── */}
          {mainTab === 'editor' && (
            <div className="flex-1 overflow-hidden">
              <WaveformEditor audioContext={audioContext} />
            </div>
          )}

        </div>{/* end center */}

        {/* ══════════════════════════════════
            RIGHT — OUTPUT ROUTER
        ══════════════════════════════════ */}
        <div className="w-[232px] shrink-0 flex flex-col overflow-y-auto border-l border-slate-800" style={{background:'#0D1428'}}>
          <div className="px-3 py-2 border-b border-slate-800 shrink-0">
            <span className="text-[9px] font-bold tracking-[0.2em] text-slate-500 uppercase">Output Router</span>
          </div>
          <div className="p-3 space-y-3 flex-1">

            {/* Monitor */}
            <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
              <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{background:'rgba(255,255,255,0.03)'}}>
                <span className="text-[9px] font-bold tracking-[0.1em] text-slate-500 uppercase flex items-center gap-1"><Headphones size={9}/> Monitor</span>
                <button onClick={() => setIsMonitoring(!isMonitoring)} disabled={recordingState === 'review'}
                  className={`w-5 h-5 rounded flex items-center justify-center border transition-all ${isMonitoring && recordingState !== 'review' ? 'border-[#00E676]/40' : 'border-slate-700'}`}
                  style={isMonitoring && recordingState !== 'review' ? {background:'rgba(0,230,118,0.15)'} : {}}>
                  <Headphones size={9} className={isMonitoring && recordingState !== 'review' ? 'text-[#00E676]' : 'text-slate-600'}/>
                </button>
              </div>
              <div className="p-2.5">
                <p className="text-[10px] text-slate-300 font-medium truncate">{monitorLabel}</p>
                <p className="text-[9px] text-slate-600 mt-0.5">{isMonitoring && recordingState !== 'review' ? 'Active — hearing processed audio' : 'Muted'}</p>
              </div>
            </div>

            {/* Broadcast */}
            <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
              <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{background:'rgba(255,255,255,0.03)'}}>
                <span className="text-[9px] font-bold tracking-[0.1em] text-slate-500 uppercase flex items-center gap-1"><Radio size={9}/> Broadcast</span>
                <div className={`w-2 h-2 rounded-full ${isLive ? 'bg-[#00E676] animate-pulse' : 'bg-slate-700'}`}/>
              </div>
              <div className="p-2.5">
                <p className="text-[10px] text-slate-300 font-medium truncate">{broadcastLabel}</p>
                <button onClick={cycleOutputTarget} className="mt-1 flex items-center gap-1.5 text-[9px] text-[#F59E0B] hover:text-amber-300 transition-colors">
                  {outputTargets.find(t => t.name === outputTarget)?.icon || <Cpu size={10}/>}
                  <span>{outputTarget}</span>
                </button>
              </div>
            </div>

            {/* Stream Guard */}
            <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
              <div className="px-3 py-1.5 border-b border-slate-800" style={{background:'rgba(255,255,255,0.03)'}}>
                <span className="text-[9px] font-bold tracking-[0.1em] text-slate-500 uppercase">Stream Guard</span>
              </div>
              <div className="p-2.5 space-y-1.5 text-[9px]">
                <div className="flex items-center justify-between">
                  <span className="text-slate-500">Limiter</span>
                  <span className={`font-bold ${features.streamingSafe ? 'text-[#00E676]' : 'text-slate-600'}`}>{features.streamingSafe ? 'ON' : 'OFF'}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-500">Target</span>
                  <span className="font-mono text-[#F59E0B]">{loudnessTarget} LUFS</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-500">Current</span>
                  <span className={`font-mono ${typeof loudnessDb === 'number' && loudnessDb > loudnessTarget + 3 ? 'text-[#FF5252]' : 'text-slate-400'}`}>{typeof loudnessDb === 'number' ? `${loudnessDb} dB` : '--'}</span>
                </div>
              </div>
            </div>

            {/* DSP Signal Flow */}
            <div className="rounded-lg border border-slate-800 overflow-hidden" style={{background:'#080E1F'}}>
              <div className="px-3 py-1.5 border-b border-slate-800" style={{background:'rgba(255,255,255,0.03)'}}>
                <span className="text-[9px] font-bold tracking-[0.1em] text-slate-500 uppercase">Signal Flow</span>
              </div>
              <div className="p-2 space-y-0">
                {[
                  {label:'MIC INPUT', active: isLive || isPlayingFile},
                  {label:'RNNOISE', active: isLive && features.denoise},
                  {label:'HYPER GATE', active: isLive && features.denoise},
                  {label:'VOICE CLEANSE', active: isLive && features.voicePattern},
                  {label:'AUTO MIX', active: isLive && features.smartMixing},
                  {label:'WARMTH', active: isLive && features.sermonWarmth},
                  {label:'ISOLATION', active: isLive && features.pastorIsolation},
                  {label:'VOICE POLISH', active: isLive && features.mastering},
                  {label:'LIMITER', active: isLive && features.streamingSafe},
                  {label:'OUTPUT', active: isLive || isPlayingFile},
                ].map(({label, active}, i, arr) => (
                  <div key={label}>
                    <div className="flex items-center gap-1.5 py-0.5">
                      <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${active ? 'bg-[#00E676]' : 'bg-slate-700'}`}/>
                      <span className={`text-[9px] font-mono ${active ? 'text-slate-300' : 'text-slate-600'}`}>{label}</span>
                      {active && voiceActive && label !== 'MIC INPUT' && label !== 'OUTPUT' && (
                        <div className="ml-auto w-1 h-1 rounded-full bg-[#00E676] animate-pulse"/>
                      )}
                    </div>
                    {i < arr.length - 1 && <div className="w-px h-1.5 bg-slate-800 ml-[5px]"/>}
                  </div>
                ))}
              </div>
            </div>

          </div>
        </div>{/* end right output router */}

      </div>{/* end main 3-panel */}

      {/* ─── MASTER CONSOLE ─── */}
      <div className="h-[72px] shrink-0 border-t border-slate-800 px-4 flex items-center gap-4 z-20" style={{background:'#030710'}}>
        {/* Transport button */}
        <button onClick={mode === 'live' ? toggleLive : null}
          className="w-11 h-11 rounded-full flex items-center justify-center transition-all"
          style={isLive
            ? {background:'#FF5252', boxShadow:'0 0 20px rgba(255,82,82,0.4)'}
            : {background:'#F59E0B', boxShadow:'0 0 20px rgba(245,158,11,0.4)'}}>
          {isLive ? <Square size={16} fill="currentColor"/> : <Play size={16} fill="currentColor" className="ml-0.5"/>}
        </button>
        {/* Status */}
        <div className="min-w-[100px]">
          <div className="text-[8px] font-bold uppercase tracking-wider text-slate-600 mb-0.5">STATUS</div>
          <div className={`text-[10px] font-semibold flex items-center gap-1.5 ${isLive ? 'text-[#00E676]' : 'text-slate-500'}`}>
            {isLive && recordingState === 'review' ? <span className="text-amber-400">Reviewing</span>
              : isLive ? <><span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{background:'#00E676'}}/>Live Processing</>
              : 'Standby'}
          </div>
        </div>
        <div className="h-9 w-px bg-slate-800"/>
        {/* Meters */}
        <div className="flex items-center gap-2">
          <div className="flex flex-col items-center gap-1">
            <div className="flex items-end gap-0.5 h-9">
              {[...Array(8)].map((_,i) => (
                <div key={i} className={`w-1.5 rounded-sm transition-all duration-75`}
                  style={{height:`${(i+1)*12}%`, background: isLive ? (i < 5 ? '#00E676' : i < 7 ? '#FFB800' : '#FF5252') : '#1e293b'}}/>
              ))}
            </div>
            <span className="text-[8px] text-slate-600" style={{fontFamily:'JetBrains Mono, monospace'}}>IN L</span>
          </div>
          <div className="flex flex-col items-center gap-1">
            <div className="flex items-end gap-0.5 h-9">
              {[...Array(8)].map((_,i) => (
                <div key={i} className="w-1.5 rounded-sm transition-all duration-75"
                  style={{height:`${(i+1)*12}%`, background: isLive && recordingState !== 'review' ? (isBypassed ? '#64748b' : i < 6 ? '#F59E0B' : '#FF5252') : '#1e293b'}}/>
              ))}
            </div>
            <span className="text-[8px] text-slate-600" style={{fontFamily:'JetBrains Mono, monospace'}}>OUT L</span>
          </div>
        </div>
        <div className="h-9 w-px bg-slate-800"/>
        {/* Gain Rider */}
        <div>
          <div className="text-[8px] font-bold uppercase tracking-wider text-slate-600 mb-0.5">Gain Rider</div>
          <span className={`text-[11px] font-mono ${features.smartMixing ? 'text-[#F59E0B]' : 'text-slate-700'}`} style={{fontFamily:'JetBrains Mono, monospace'}}>
            {features.smartMixing ? `${gainRiderDb >= 0 ? '+' : ''}${gainRiderDb.toFixed(1)} dB` : 'OFF'}
          </span>
        </div>
        <div className="h-9 w-px bg-slate-800"/>
        {/* Controls — right side */}
        <div className="flex items-center gap-2 ml-auto flex-wrap">
          <button onClick={() => setIsBypassed(!isBypassed)}
            className="px-2.5 py-1.5 rounded-lg border flex items-center gap-1.5 text-[9px] font-bold transition-all"
            style={isBypassed ? {background:'rgba(255,82,82,0.15)', borderColor:'rgba(255,82,82,0.4)', color:'#FF5252'} : {background:'#0D1428', borderColor:'#1e293b', color:'#94a3b8'}}>
            <Power size={12}/>{isBypassed ? 'BYPASSED' : 'BYPASS AI'}
          </button>
          <button onClick={() => setIsMonitoring(!isMonitoring)} disabled={recordingState === 'review'}
            className="p-1.5 rounded-lg border relative transition-all"
            style={isMonitoring ? {background:'rgba(245,158,11,0.15)', borderColor:'rgba(245,158,11,0.4)', color:'#F59E0B'} : {background:'#0D1428', borderColor:'#1e293b', color:'#94a3b8'}}>
            <Headphones size={16}/>
            {isMonitoring && recordingState !== 'review' && <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full animate-pulse" style={{background:'#00E676'}}/>}
          </button>
          {isLive && (
            <button onClick={downloadWaveform} className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg border text-[9px] font-semibold hover:opacity-80"
              style={{background:'#0D1428', borderColor:'rgba(34,211,238,0.3)', color:'#22d3ee'}}>
              <Activity className="w-3 h-3"/> Snapshot
            </button>
          )}
          {isLive && (
            <button onClick={exportMp4} disabled={exportStatus === 'sharing'} className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg border text-[9px] font-semibold hover:opacity-80 disabled:opacity-40"
              style={{background:'#0D1428', borderColor:'rgba(168,85,247,0.4)', color:'#c084fc'}}
              title="Record 10s of canvas + processed audio as MP4/WebM">
              {exportStatus === 'sharing' ? <span className="w-2.5 h-2.5 rounded-full border border-purple-400 border-t-transparent animate-spin"/> : <Video className="w-3 h-3"/>}
              {exportStatus === 'done' ? 'Saved!' : exportStatus === 'sharing' ? 'Recording...' : 'MP4'}
            </button>
          )}
          <button onClick={hardReset} className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg border text-[9px] font-semibold hover:opacity-80"
            style={{background:'#0D1428', borderColor:'rgba(245,158,11,0.3)', color:'#fbbf24'}}>
            <RefreshCw className="w-3 h-3"/> Reset
          </button>
          {mode === 'file' && fileInfo && (
            <button onClick={processAndExportFile} disabled={fileExportStatus === 'processing' || !destNodeRef.current || !destNodeRef.current.stream}
              className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg border text-[9px] font-semibold hover:opacity-80 disabled:opacity-30"
              style={{background:'#0D1428', borderColor:'rgba(0,230,118,0.3)', color:'#00E676'}}>
              {fileExportStatus === 'processing' ? <span className="w-2.5 h-2.5 rounded-full border border-[#00E676] border-t-transparent animate-spin"/> : fileExportStatus === 'done' ? <Check className="w-3 h-3"/> : <Upload className="w-3 h-3"/>}
              {fileExportStatus === 'done' ? 'Mastered' : fileExportStatus === 'processing' ? 'Processing...' : 'Process & Export'}
            </button>
          )}
          <button onClick={() => setShowSettings(true)} className="p-1.5 rounded-lg border transition-all hover:border-slate-600"
            style={{background:'#0D1428', borderColor:'#1e293b', color:'#94a3b8'}}>
            <Settings size={14}/>
          </button>
        </div>
      </div>{/* end master console */}

      {/* Mobile fallback for very small screens */}
      <div className="lg:hidden fixed bottom-0 left-0 right-0 z-40 border-t border-slate-800 px-3 py-2 flex justify-between items-center" style={{background:'#030710'}}>
        <button onClick={mode === 'live' ? toggleLive : null} className="w-10 h-10 rounded-full flex items-center justify-center"
          style={isLive ? {background:'#FF5252'} : {background:'#F59E0B'}}>
          {isLive ? <Square size={14} fill="currentColor"/> : <Play size={14} fill="currentColor"/>}
        </button>
        <span className={`text-[10px] font-semibold ${isLive ? 'text-[#00E676]' : 'text-slate-500'}`}>{isLive ? 'Live' : 'Standby'}</span>
        <button onClick={() => setIsBypassed(!isBypassed)} className={`px-2 py-1 rounded text-[9px] font-bold border ${isBypassed ? 'border-red-500/50 text-red-400' : 'border-slate-700 text-slate-500'}`}>
          {isBypassed ? 'BYPASS' : 'AI ON'}
        </button>
        <button onClick={() => setShowSettings(true)} className="p-2 text-slate-400"><Settings size={16}/></button>
      </div>

      {/* AI Assistant floating button */}
      <button
        onClick={() => setShowAI(p => !p)}
        title="TIWATON AI — Claude powered"
        className="fixed bottom-4 left-4 z-50 w-10 h-10 rounded-full flex items-center justify-center shadow-lg border transition-all hover:scale-105"
        style={showAI
          ? { background: '#F59E0B', borderColor: '#F59E0B' }
          : { background: '#0D1428', borderColor: 'rgba(245,158,11,0.4)' }}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path d="M8 1.5C4.41 1.5 1.5 4.41 1.5 8s2.91 6.5 6.5 6.5 6.5-2.91 6.5-6.5S11.59 1.5 8 1.5zm0 2a1 1 0 110 2 1 1 0 010-2zm1 8H7v-4h2v4z"
            fill={showAI ? '#fff' : '#F59E0B'}/>
        </svg>
      </button>

      {/* AI Assistant panel */}
      <AIAssistant
        isOpen={showAI}
        onClose={() => setShowAI(false)}
        isLive={isLive}
        features={features}
        setFeatures={setFeatures}
        currentPreset={speakerPreset}
        spectrumData={null}
      />

    </div>
  );   // ✅ end of JSX returned from AudioProcessor
 }; // ✅ end of: const AudioProcessor = ({ goHome }) => {


// --- Small UI helpers ---
const FeatureToggle = ({ icon, label, desc, active, onClick }) => (
  <div
    onClick={onClick}
    className={`p-3 rounded-xl border cursor-pointer transition-all group ${
      active
        ? 'bg-amber-900/30 border-amber-500/50 shadow-[0_0_15px_rgba(245,158,11,0.2)]'
        : 'bg-slate-900 border-slate-800 hover:border-slate-700 opacity-60 hover:opacity-100'
    }`}
  >
    <div className="flex items-center justify-between mb-1">
      <div
        className={`flex items-center gap-3 ${
          active ? 'text-amber-400' : 'text-slate-400 group-hover:text-slate-300'
        }`}
      >
        {icon}
        <span className="font-semibold text-sm">{label}</span>
      </div>
      <div
        className={`w-4 h-4 rounded-full border flex items-center justify-center ${
          active ? 'bg-amber-500 border-amber-500' : 'border-slate-600'
        }`}
      >
        {active && <CheckCircle2 size={12} className="text-white" />}
      </div>
    </div>
    <p className="text-xs text-slate-500 pl-8">{desc}</p>
  </div>
);

const Meter = ({ label, level, color = 'bg-green-500' }) => (
  <div className="flex flex-col items-center gap-1">
    <div className="flex items-end gap-0.5 h-6 md:h-10">
      {[...Array(5)].map((_, i) => (
        <div
          key={i}
          className={`w-1 md:w-1.5 rounded-sm transition-all ${
            level > i * 20 ? color : 'bg-slate-800'
          }`}
          style={{ height: `${(i + 1) * 20}%` }}
        />
      ))}
    </div>
    <span className="text-[9px] md:text-[10px] text-slate-500 font-mono scale-90 md:scale-100">
      {label}
    </span>
  </div>
);

const Badge = ({ active, text }) => (
  <div
    className={`px-2 py-1 rounded text-[10px] font-bold tracking-wider border ${
      active
        ? 'bg-green-500/10 text-green-400 border-green-500/30'
        : 'bg-slate-800 text-slate-600 border-transparent'
    }`}
  >
    {text}
  </div>
);

function HelpCorner() {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <>
      {/* Floating Help Button */}
      <button
        type="button"
        data-help-button
        onClick={() => setIsOpen(true)}
        className="fixed bottom-20 right-4 z-40 flex items-center gap-2 rounded-full bg-slate-900/95 px-4 py-2 text-xs font-semibold text-slate-100 shadow-lg border border-slate-700 hover:bg-slate-800"
      >
        <span className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-amber-500 text-[11px] font-bold">
          ?
        </span>
        <span className="hidden sm:inline">Need help with audio?</span>
        <span className="sm:hidden">Help</span>
      </button>

      {/* Help Panel */}
      {isOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm px-4">
          <div className="flex max-h-[80vh] w-full max-w-lg flex-col rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
            {/* Header */}
            <div className="flex items-center justify-between border-b border-slate-800 px-4 py-3">
              <div className="flex items-center gap-2">
                <div className="flex h-7 w-7 items-center justify-center rounded-full bg-amber-600 text-xs font-bold text-white">
                  ?
                </div>
                <div>
                  <p className="text-sm font-semibold text-slate-100">
                    TIWATON Sunday Service Assistant
                  </p>
                  <p className="text-[11px] text-slate-400">
                    Routing, setup & quick fixes for clean livestream audio.
                  </p>
                </div>
              </div>

              <button
                onClick={() => setIsOpen(false)}
                className="rounded-full p-1.5 text-slate-400 hover:bg-slate-800 hover:text-white"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            {/* Body – upgraded routing guide */}
            <div className="flex-1 space-y-4 overflow-y-auto px-4 py-3 text-[11px] leading-relaxed text-slate-300">
              {/* SECTION: Title */}
              <p className="font-semibold text-slate-100 text-sm">
                🎚 TIWATON + OBS Routing Guide
              </p>
              <p className="text-[11px] text-slate-400">
                Follow this quick workflow every Sunday to get clean, consistent livestream audio.
              </p>

              {/* SECTION 1: Physical Setup */}
              <div className="rounded-xl border border-slate-800 bg-slate-900 px-3 py-2 space-y-1">
                <p className="font-semibold text-slate-100">1️⃣ Physical Setup</p>
                <ul className="ml-4 list-disc space-y-1">
                  <li>Pastor mic → Mixer / Audio Interface (correct gain, no clipping).</li>
                  <li>
                    Only <span className="font-semibold text-amber-300">one</span> clean feed to the computer (USB preferred).
                  </li>
                  <li>Turn off system Noise Suppression / AGC.</li>
                  <li>Sound engineer monitors with AirPods / headphones.</li>
                </ul>
              </div>

              {/* SECTION 2: TIWATON INPUT */}
              <div>
                <p className="font-semibold text-slate-100">2️⃣ Inside TIWATON Studio</p>

                <div className="mt-1 ml-4">
                  <p className="font-semibold text-slate-200">A. Input Source</p>
                  <ul className="ml-4 list-disc space-y-1">
                    <li>Select mixer or USB interface (e.g. Scarlett, Behringer, Yamaha AG06).</li>
                    <li>Confirm levels are healthy but not clipping.</li>
                  </ul>
                </div>

                <div className="mt-2 ml-4">
                  <p className="font-semibold text-slate-200">B. Outputs</p>
                  <ul className="ml-4 list-disc space-y-1">
                    <li>
                      <span className="text-amber-300 font-semibold">Monitoring Output</span>{' '}
                      → AirPods / Headphones (what you listen on).
                    </li>
                    <li>
                      <span className="text-amber-300 font-semibold">Broadcast Output</span>{' '}
                      → Virtual Cable (what OBS / YouTube hears).
                    </li>
                    <li>
                      Footer will show something like:
                      <br />
                      <span className="text-slate-400 ml-4">
                        Monitor: AirPods Max · Broadcast: VB-CABLE Input
                      </span>
                    </li>
                  </ul>
                </div>

                <div className="mt-2 ml-4">
                  <p className="font-semibold text-slate-200">C. Processing Checklist</p>
                  <ul className="ml-4 list-disc space-y-1">
                    <li>Enable Hyper-Gate, Voice Isolation, Warmth, Auto-Mixing.</li>
                    <li>
                      Run{' '}
                      <span className="font-semibold text-amber-300">
                        Auto-Calibrate Gate
                      </span>{' '}
                      once when the room is at normal noise level.
                    </li>
                    <li>
                      Use <span className="font-semibold">Record Check</span> to confirm clarity
                      before going live.
                    </li>
                  </ul>
                </div>
              </div>

              {/* SECTION 3: OBS Setup */}
              <div>
                <p className="font-semibold text-slate-100">3️⃣ In OBS / Streaming Apps</p>

                <ul className="ml-4 list-disc space-y-1 mt-1">
                  <li>
                    Add{' '}
                    <span className="font-semibold text-amber-300">
                      Audio Input Capture
                    </span>{' '}
                    → Choose:{' '}
                    <span className="font-semibold">VB-CABLE / BlackHole / Loopback</span>.
                  </li>
                  <li>
                    Remove all raw mics (webcam, laptop mic, mixer direct, built-in mic,
                    etc).
                  </li>
                  <li>
                    Rule:
                    <br />
                    <span className="ml-4 text-amber-300 font-semibold">
                      If TIWATON says “Broadcast: VB-CABLE”, OBS must also use VB-CABLE.
                    </span>
                  </li>
                </ul>
              </div>

              {/* SECTION 4: Troubleshooting */}
              <div>
                <p className="font-semibold text-slate-100">4️⃣ Troubleshooting (Live)</p>

                <ul className="ml-4 list-disc space-y-1 mt-1">
                  <li>
                    <span className="text-red-400 font-semibold">
                      No sound on stream?
                    </span>{' '}
                    Check Live is ON and OBS is using the Virtual Cable device.
                  </li>
                  <li>
                    <span className="text-red-400 font-semibold">
                      Echo / double audio?
                    </span>{' '}
                    Two mics active. In OBS, keep only the virtual cable input.
                  </li>
                  <li>
                    <span className="text-amber-300 font-semibold">
                      Gate too strong?
                    </span>{' '}
                    Lower Noise Threshold in TIWATON (move the slider left).
                  </li>
                  <li>
                    <span className="text-amber-300 font-semibold">
                      TIWATON acting strange?
                    </span>{' '}
                    Use Reset in the footer and restart Live.
                  </li>
                </ul>
              </div>

              {/* GOLDEN RULE CARD */}
              <div className="mt-2 rounded-xl border border-slate-800 bg-slate-900 px-3 py-2">
                <p className="text-[11px] text-slate-400">
                  <span className="font-semibold text-amber-300">Golden Rule:</span>{' '}
                  TIWATON replaces all mics in OBS. Only the{' '}
                  <span className="font-semibold text-amber-300">Broadcast Output</span> from
                  TIWATON should be selected as the mic in OBS / streaming apps.
                </p>
              </div>
            </div>

            {/* Footer */}
            <div className="flex items-center justify-between border-t border-slate-800 px-4 py-2 text-[10px] text-slate-500">
              <span>Checklist tuned for your TIWATON studio.</span>
              <span className="hidden sm:inline">Your audio co-pilot is here.</span>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

export default TiwatonApp;
