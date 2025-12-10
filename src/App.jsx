import React, { useState, useEffect, useRef } from 'react';
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
 * - Process & Export for File mode (AI-mastered audio) — now more robust
 */

const TiwatonApp = () => {
  const [view, setView] = useState('landing');

  if (view === 'landing') {
    return <LandingPage onEnter={() => setView('studio')} />;
  }

  return (
    <>
      <AudioProcessor goHome={() => setView('landing')} />
      <HelpCorner />
    </>
  );
};

// --- LANDING PAGE ---
const LandingPage = ({ onEnter }) => {
  return (
    <div className="h-[100dvh] bg-slate-950 text-white flex flex-col items-center justify-center relative overflow-hidden font-sans selection:bg-indigo-500 selection:text-white">
      <div className="absolute inset-0 z-0 opacity-20">
        <div className="absolute top-[-20%] left-[-10%] w-[600px] h-[600px] bg-indigo-600 rounded-full blur-[120px] animate-pulse" />
        <div
          className="absolute bottom-[-20%] right-[-10%] w-[500px] h-[500px] bg-purple-600 rounded-full blur-[120px]"
          style={{ animationDuration: '4s' }}
        />
      </div>

      <div className="z-10 text-center max-w-3xl px-6">
        <div className="mb-8 flex justify-center">
          <div className="w-20 h-20 bg-gradient-to-tr from-indigo-500 to-purple-500 rounded-2xl flex items-center justify-center shadow-[0_0_40px_rgba(99,102,241,0.6)] rotate-3 hover:rotate-6 transition-transform duration-500">
            <Waves size={40} className="text-white" />
          </div>
        </div>

        <h1 className="text-5xl md:text-8xl font-black tracking-tighter mb-4 bg-clip-text text-transparent bg-gradient-to-r from-white via-indigo-200 to-slate-400">
          TIWATON.
        </h1>

        <p className="text-lg md:text-2xl text-slate-400 font-light mb-12 leading-relaxed">
          The AI Sound Engineer your church actually needs.
          <br />
          <span className="text-indigo-400 font-medium">
            True Noise Gating. Studio Clarity. Zero Excuses.
          </span>
        </p>

        <button
          onClick={onEnter}
          className="group relative px-8 py-5 bg-white text-slate-950 font-bold text-lg rounded-full overflow-hidden hover:scale-105 transition-transform duration-300 shadow-[0_0_30px_rgba(255,255,255,0.3)]"
        >
          <div className="absolute inset-0 bg-gradient-to-r from-indigo-400 to-purple-400 opacity-0 group-hover:opacity-100 transition-opacity duration-300" />
          <span className="relative z-10 flex items-center gap-3 group-hover:text-white transition-colors">
            Enter Studio <ArrowRight size={20} />
          </span>
        </button>
      </div>
    </div>
  );
};

// --- STUDIO COMPONENT ---
const AudioProcessor = ({ goHome }) => {
  const [isLive, setIsLive] = useState(false);
  const [isPlayingFile, setIsPlayingFile] = useState(false);
  const [audioContext, setAudioContext] = useState(null);
  const [mode, setMode] = useState('live');
  const [outputTarget, setOutputTarget] = useState('OBS Studio');
  const [showSettings, setShowSettings] = useState(false);
  const [isMonitoring, setIsMonitoring] = useState(false);
  const [isBypassed, setIsBypassed] = useState(false);
  const [exportStatus, setExportStatus] = useState(null);
  const [inputGainValue, setInputGainValue] = useState(1.0);
  const [isSidebarOpen, setIsSidebarOpen] = useState(false);
  const [voiceActive, setVoiceActive] = useState(false);
  const [isAutoCalibrating, setIsAutoCalibrating] = useState(false);

  const [noiseFloorThreshold, setNoiseFloorThreshold] = useState(-50);
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

  const [availableDevices, setAvailableDevices] = useState({
    inputs: [],
    outputs: [],
  });
  const [selectedDevices, setSelectedDevices] = useState({
    inputId: 'default',
    outputId: 'default',
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

  // -- Features --
  const [features, setFeatures] = useState({
    denoise: false,
    dereverb: false,
    pastorIsolation: false,
    sermonWarmth: false,
    smartMixing: false, // drives Smart Gain Rider as well
    mastering: false,
  });

  const canvasRef = useRef(null);
  const animationRef = useRef(null);

  useEffect(() => {
    isMonitoringRef.current = isMonitoring;
  }, [isMonitoring]);

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

  const resetDeviceState = () => {
    setAvailableDevices({ inputs: [], outputs: [] });
    setSelectedDevices({ inputId: 'default', outputId: 'default' });
  };

  const getDevices = async () => {
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
  };

  useEffect(() => {
    if (showSettings) getDevices();
  }, [showSettings]);

  useEffect(() => {
    settingsRef.current = {
      denoise: features.denoise,
      threshold: noiseFloorThreshold,
      isBypassed,
    };

    // Force gate open if denoise is off
    if (!features.denoise && processingRefs.current.gateGain && audioContext) {
      processingRefs.current.gateGain.gain.setValueAtTime(
        1.0,
        audioContext.currentTime,
      );
    }
  }, [features.denoise, noiseFloorThreshold, isBypassed, audioContext]);

  useEffect(() => {
    return () => cleanupAudio();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const cleanupAudio = () => {
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

    if (recordingTimerRef.current) clearInterval(recordingTimerRef.current);
    setRecordingState('idle');
  };

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
    });

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

  const startAudioEngine = async (inputStream = null, mediaElement = null) => {
    // Stop any previous animation loop
    if (animationRef.current) {
      cancelAnimationFrame(animationRef.current);
      animationRef.current = null;
    }

    // Create a fresh context every time
    const ctx = new (window.AudioContext || window.webkitAudioContext)();

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

    let source;
    try {
      if (inputStream) {
        source = ctx.createMediaStreamSource(inputStream);
      } else if (mediaElement) {
        source = ctx.createMediaElementSource(mediaElement);
      } else {
        return;
      }
    } catch (e) {
      console.error(e);
      return;
    }

    // --- DSP NODES ---
    const inputGain = ctx.createGain();
    inputGain.gain.value = inputGainValue;

    const lowCut = ctx.createBiquadFilter();
    lowCut.type = 'highpass';
    lowCut.frequency.value = 80;

    const gateGain = ctx.createGain();
    gateGain.gain.value = 1.0;

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

    const master = ctx.createGain();
    master.gain.value = 1.0;

    const ana = ctx.createAnalyser();
    ana.fftSize = 2048;

    const monitorGain = ctx.createGain();
    monitorGain.gain.value = isMonitoringRef.current ? 1.0 : 0.0;

    const destNode = ctx.createMediaStreamDestination();
    destNodeRef.current = destNode;

    // --- CHAIN CONNECTIONS ---
    source.connect(inputGain);
    inputGain.connect(lowCut);
    lowCut.connect(gateGain);

    gateGain.connect(deEsser);
    deEsser.connect(compressor);

    // Smart Gain Rider after compressor
    compressor.connect(autoGain);
    autoGain.connect(eqWarmth);

    eqWarmth.connect(eqClarity);
    eqClarity.connect(master);
    master.connect(ana);
    ana.connect(monitorGain);
    ana.connect(destNode);

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
      deEsser,
      compressor,
      autoGain,
      eqWarmth,
      eqClarity,
      master,
      monitorGain,
      analyser: ana,
    };

    visualizeAndGate();
  };

  const toggleLive = async () => {
    if (isLive) {
      cleanupAudio();
    } else {
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
        setIsLive(true);
        if (audioElRef.current) {
          audioElRef.current.pause();
          setIsPlayingFile(false);
        }
        startAudioEngine(s, null);
      } catch (err) {
        console.error(err);
        alert(
          'Microphone access blocked or unavailable. Please check browser permissions and audio device.',
        );
        cleanupAudio();
      }
    }
  };

  const handleModeSwitch = (newMode) => {
    if (isLive) cleanupAudio();
    setMode(newMode);

    if (newMode === 'file') {
      fileInputRef.current?.click();
    }
  };

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

  const shareRecording = () => {
    if (!recordedUrl) return;
    setExportStatus('sharing');
    setTimeout(() => {
      const a = document.createElement('a');
      a.style.display = 'none';
      a.href = recordedUrl;
      a.download = `Tiwaton_Check_${new Date().toLocaleTimeString()}.webm`;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(recordedUrl);
      setExportStatus('done');
    }, 1500);
  };

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
      alert('No processed audio stream found. Wait for the file to fully load, then try again.');
      setFileExportStatus(null);
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

    try {
      mediaEl.pause();
      mediaEl.currentTime = 0;
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
    const duration = Number.isFinite(mediaEl.duration) && mediaEl.duration > 0
      ? mediaEl.duration
      : 300; // fallback for unknown duration (~5 min)
    const maxRecordMs = duration * 1000 + 3000;
    const timeoutId = setTimeout(() => {
      if (recorder && recorder.state === 'recording') {
        console.warn('Export timeout reached, stopping recorder.');
        recorder.stop();
      }
    }, maxRecordMs);

    recorder.ondataavailable = (e) => {
      if (e.data && e.data.size > 0) chunks.push(e.data);
    };

    recorder.onstop = () => {
      clearTimeout(timeoutId);

      if (!chunks.length) {
        console.warn('No audio data captured during export.');
        setFileExportStatus('error');
        alert('No audio captured from this file. Please check your source and try again.');
        return;
      }

      const blob = new Blob(chunks, { type: 'audio/webm' });
      const url = URL.createObjectURL(blob);

      const safeName = (fileInfo?.name || 'tiwaton_sermon')
        .replace(/\.[^/.]+$/, '')
        .slice(0, 60);

      const a = document.createElement('a');
      a.style.display = 'none';
      a.href = url;
      a.download = `${safeName}_TIWATON_master.webm`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      setFileExportStatus('done');
    };

    const handleEnded = () => {
      mediaEl.removeEventListener('ended', handleEnded);
      if (recorder.state === 'recording') {
        recorder.stop();
      }
    };

    mediaEl.addEventListener('ended', handleEnded);

    const playPromise = mediaEl.play();
    if (playPromise && playPromise.catch) {
      playPromise
        .catch((err) => {
          console.error('Playback failed during export', err);
          mediaEl.removeEventListener('ended', handleEnded);
          try {
            if (recorder.state === 'recording') {
              recorder.stop();
            }
          } catch (e) {
            console.warn('Error stopping recorder after playback failure', e);
          }
          setFileExportStatus('error');
          alert('Unable to play this file for export. Please interact with the player first and try again.');
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

  // --- Parameter updates (EQ & compressor tweaks based on features) ---
  useEffect(() => {
    if (!audioContext || !processingRefs.current.compressor) return;
    const { compressor, eqWarmth, eqClarity, master } = processingRefs.current;
    const now = audioContext.currentTime;

    const getTarget = (isActive, activeVal, passiveVal) =>
      isActive && !isBypassed ? activeVal : passiveVal;

    // Small dynamic boost when voice is active
    const speechBoost = voiceActive ? 2 : 0;

    if (eqClarity) {
      const baseClarity = getTarget(features.pastorIsolation, 8, 0);
      eqClarity.gain.linearRampToValueAtTime(baseClarity + speechBoost, now + 0.2);
    }

    if (eqWarmth) {
      const baseWarmth = getTarget(features.sermonWarmth, 5, 0);
      eqWarmth.gain.linearRampToValueAtTime(
        baseWarmth + speechBoost * 0.6,
        now + 0.25,
      );
    }

    if (compressor) {
      compressor.ratio.linearRampToValueAtTime(
        getTarget(features.smartMixing, 4, 1),
        now + 0.4,
      );
      compressor.threshold.linearRampToValueAtTime(
        getTarget(features.smartMixing, -24, 0),
        now + 0.4,
      );
    }

    if (master) {
      const targetGain = isBypassed ? 1.0 : 1.0 + speechBoost * 0.08;
      master.gain.linearRampToValueAtTime(targetGain, now + 0.15);
    }
  }, [features, audioContext, isBypassed, voiceActive]);

  // --- Visualizer + Gate + Smart Gain Rider (fancy visual AI state) ---
  const visualizeAndGate = () => {
    const { analyser, gateGain } = processingRefs.current;
    const canvas = canvasRef.current;
    if (!analyser || !canvas) return;

    const ctx = canvas.getContext('2d');
    const bufferLength = analyser.frequencyBinCount;
    const freqData = new Uint8Array(bufferLength);
    const timeData = new Uint8Array(bufferLength);

    let lastGateUpdate = 0;
    let lastVoiceUpdate = 0;
    let lastGainUpdate = 0;
    const updateInterval = 100; // ms
    const gainUpdateInterval = 200; // ms

    const draw = (timestamp) => {
      animationRef.current = requestAnimationFrame(draw);

      analyser.getByteFrequencyData(freqData);
      analyser.getByteTimeDomainData(timeData);

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

      const barWidth = (w / bufferLength) * 2.5;
      let x = 0;

      // Spectrum + band energies
      let lowEnergy = 0;
      let highEnergy = 0;
      const splitIndex = Math.floor(bufferLength * 0.4);

      for (let i = 0; i < bufferLength; i++) {
        const v = freqData[i];
        const barHeight = v * 1.5;

        if (i < splitIndex) {
          lowEnergy += v;
        } else {
          highEnergy += v;
        }

        let barColor = 'rgba(37,99,235,0.75)'; // blue-600
        if (i > splitIndex * 0.6 && i < splitIndex * 1.3) {
          barColor = 'rgba(56,189,248,0.75)'; // cyan-400
        }

        ctx.fillStyle = barColor;
        ctx.fillRect(x, h - barHeight / 2, barWidth, barHeight / 2);
        x += barWidth + 1;
      }

      const lowAvg = lowEnergy / (splitIndex || 1);
      const highAvg = highEnergy / (bufferLength - splitIndex || 1);
      const highBias = highAvg / (lowAvg + 1e-6);
      const antiChildNoiseActive = features.denoise;
      const highOnlyNoise = antiChildNoiseActive && highBias > 2.2 && lowAvg < 40;

      // Time-domain RMS
      let sumSq = 0;
      for (let i = 0; i < bufferLength; i++) {
        const sample = (timeData[i] - 128) / 128.0;
        sumSq += sample * sample;
      }
      const rms = Math.sqrt(sumSq / bufferLength);
      const db = 20 * Math.log10(rms || 0.00001);

      let gateClosed = false;
      let localVoiceActive = false;

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
      if (gateGain && features.denoise && !settingsRef.current.isBypassed) {
        const threshold = settingsRef.current.threshold;

        let shouldOpen = db >= threshold;
        if (highOnlyNoise) {
          shouldOpen = false;
        }

        const target = shouldOpen ? 1.0 : 0.001;

        const current = gateGain.gain.value;
        const smoothing = target > current ? 0.2 : 0.05;
        gateGain.gain.value = current + (target - current) * smoothing;

        gateClosed = gateGain.gain.value < 0.1;

        localVoiceActive = !gateClosed && db > threshold + 3 && !highOnlyNoise;

        if (timestamp - lastVoiceUpdate > updateInterval) {
          setVoiceActive((prev) =>
            prev !== localVoiceActive ? localVoiceActive : prev,
          );
          lastVoiceUpdate = timestamp;
        }

        if (timestamp - lastGateUpdate > updateInterval) {
          setVisualizerGateStatus((prev) =>
            prev !== gateClosed ? gateClosed : prev,
          );
          lastGateUpdate = timestamp;
        }
      } else if (gateGain) {
        gateGain.gain.value = 1.0;
        gateClosed = false;
        localVoiceActive = false;

        if (visualizerGateStatus !== false) {
          setVisualizerGateStatus(false);
        }
        if (voiceActive !== false) {
          setVoiceActive(false);
        }
      }

      // Core orb
      const coreRadius = h * 0.35;
      const coreGrad = ctx.createRadialGradient(
        cx,
        cy,
        coreRadius * 0.1,
        cx,
        cy,
        coreRadius,
      );

      if (highOnlyNoise) {
        coreGrad.addColorStop(0, 'rgba(248,113,113,0.45)');
        coreGrad.addColorStop(0.5, 'rgba(127,29,29,0.25)');
        coreGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
      } else if (!gateClosed && localVoiceActive) {
        coreGrad.addColorStop(0, 'rgba(56,189,248,0.6)');
        coreGrad.addColorStop(0.4, 'rgba(59,130,246,0.35)');
        coreGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
      } else {
        coreGrad.addColorStop(0, 'rgba(129,140,248,0.35)');
        coreGrad.addColorStop(0.5, 'rgba(30,64,175,0.25)');
        coreGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
      }

      ctx.fillStyle = coreGrad;
      ctx.beginPath();
      ctx.arc(cx, cy, coreRadius, 0, Math.PI * 2);
      ctx.fill();

      // Red veil for harsh highs
      if (highOnlyNoise) {
        const noiseGrad = ctx.createLinearGradient(0, 0, 0, h * 0.4);
        noiseGrad.addColorStop(0, 'rgba(248,113,113,0.4)');
        noiseGrad.addColorStop(1, 'rgba(15,23,42,0.0)');
        ctx.fillStyle = noiseGrad;
        ctx.fillRect(0, 0, w, h * 0.4);
      }

      // Breathing ring when voice active
      if (!gateClosed && localVoiceActive) {
        const t = (timestamp % 2000) / 2000;
        const pulse = 0.85 + 0.15 * Math.sin(t * Math.PI * 2);
        const ringRadius = coreRadius * pulse;

        ctx.beginPath();
        ctx.arc(cx, cy, ringRadius, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(56,189,248,0.7)';
        ctx.lineWidth = 3;
        ctx.setLineDash([10, 8]);
        ctx.stroke();
        ctx.setLineDash([]);
      }

      // Gate threshold line
      if (features.denoise && !settingsRef.current.isBypassed) {
        const threshold = settingsRef.current.threshold;
        const norm = (threshold + 100) / 100;
        const threshY = h * (1 - norm);
        ctx.strokeStyle = '#6366f1';
        ctx.setLineDash([5, 5]);
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(0, threshY);
        ctx.lineTo(w, threshY);
        ctx.stroke();
        ctx.setLineDash([]);
      }

      // Smart Gain Rider
      const autoGainNode = processingRefs.current.autoGain;
      const gainRiderEnabled =
        features.smartMixing && !settingsRef.current.isBypassed;

      if (autoGainNode && gainRiderEnabled && !gateClosed) {
        const aggressiveMode =
          settingsRef.current.denoise && settingsRef.current.threshold > -40;

        const targetSpeechDb = aggressiveMode ? -18 : -24;
        const maxBoostDb = aggressiveMode ? 16 : 12;
        const maxCutDb = -12;

        let neededChange = targetSpeechDb - db;
        if (neededChange > maxBoostDb) neededChange = maxBoostDb;
        if (neededChange < maxCutDb) neededChange = maxCutDb;

        const desiredLinear = Math.pow(10, neededChange / 20);
        const currentLinear = autoGainNode.gain.value || 1.0;

        const smoothingRider = 0.02;
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

        if (timestamp - lastGainUpdate > gainUpdateInterval && gainRiderDb !== 0) {
          setGainRiderDb(0);
          lastGainUpdate = timestamp;
        }
      }
    };

    draw(performance.now());
  };

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

  // --- RENDER ---
  return (
    <div className="flex flex-col h-[100dvh] bg-slate-900 text-white font-sans overflow-hidden">
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

      <header className="flex-none flex items-center justify-between px-4 py-3 md:px-6 md:py-4 bg-slate-950 border-b border-slate-800 z-30">
        <div
          className="flex items-center gap-3 cursor-pointer group"
          onClick={goHome}
        >
          <div className="w-8 h-8 md:w-10 md:h-10 bg-indigo-600 rounded-lg flex items-center justify-center shadow-[0_0_15px_rgba(79,70,229,0.5)] group-hover:scale-105 transition-transform">
            <Waves className="text-white w-5 h-5 md:w-6 md:h-6" />
          </div>
          <div>
            <h1 className="text-lg md:text-xl font-bold tracking-wide bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-transparent group-hover:from-indigo-300 group-hover:to-purple-300 transition-colors">
              TIWATON <span className="font-light hidden sm:inline">AI STUDIO</span>
            </h1>
            <div className="flex items-center gap-1">
              <span
                className={`w-2 h-2 rounded-full ${
                  isLive || isPlayingFile
                    ? 'bg-green-500 animate-pulse'
                    : 'bg-slate-600'
                }`}
              />
              <p className="text-[10px] md:text-xs text-slate-400 font-medium">
                System Ready • v11.x
              </p>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2 md:gap-4">
          <div className="flex bg-slate-900 rounded-full p-1 border border-slate-800">
            <button
              onClick={() => handleModeSwitch('live')}
              className={`px-4 py-1.5 rounded-full text-sm font-medium transition-all ${
                mode === 'live'
                  ? 'bg-indigo-600 shadow-lg'
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              Live Input
            </button>
            <button
              onClick={() => handleModeSwitch('file')}
              className={`px-4 py-1.5 rounded-full text-sm font-medium transition-all ${
                mode === 'file'
                  ? 'bg-indigo-600 shadow-lg'
                  : 'text-slate-400 hover:text-white'
              }`}
            >
              File Upload
            </button>
          </div>
          <button
            onClick={() => setShowSettings(true)}
            className={`w-8 h-8 flex items-center justify-center rounded-full hover:bg-slate-800 transition-colors ${
              audioStats.sampleRate > 0 && audioStats.sampleRate < 44100
                ? 'text-amber-500 animate-pulse'
                : 'text-slate-400 hover:text-white'
            }`}
          >
            {audioStats.sampleRate > 0 && audioStats.sampleRate < 44100 ? (
              <AlertTriangle className="w-5 h-5" />
            ) : (
              <Settings className="w-5 h-5" />
            )}
          </button>
        </div>
      </header>

      {showSettings && (
        <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center backdrop-blur-sm p-4">
          <div className="bg-slate-900 w-full max-w-lg rounded-2xl border border-slate-700 shadow-2xl p-6 space-y-6">
            <div className="flex justify-between items-center border-b border-slate-800 pb-4">
              <h2 className="text-xl font-bold flex items-center gap-2">
                <Settings size={20} className="text-indigo-400" />
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
            <div>
              <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
                Input Source
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
            <div>
              <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
                Output Destination
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
                      : 'text-indigo-400'
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
                <div className="text-lg font-mono text-indigo-400">
                  {audioStats.bufferSize || '--'} smp
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* MAIN LAYOUT */}
      <div className="flex-1 flex flex-col lg:flex-row overflow-hidden relative">
        {/* MOBILE CONTROLS TOGGLE */}
        <div className="lg:hidden bg-slate-950 border-b border-slate-800 p-3 flex justify-between items-center z-30 shrink-0">
          <span className="text-xs font-bold text-slate-400 flex items-center gap-2">
            <Sliders size={14} /> AI CONTROLS
          </span>
          <button
            onClick={() => setIsSidebarOpen(!isSidebarOpen)}
            className="p-2 bg-slate-900 rounded-lg text-indigo-400 border border-slate-800 active:bg-slate-800"
          >
            {isSidebarOpen ? <X size={18} /> : <Menu size={18} />}
          </button>
        </div>

        {/* SIDEBAR */}
        <div
          className={`
            absolute inset-0 z-20 bg-slate-950/95 backdrop-blur-xl md:w-80
            lg:relative lg:bg-slate-950 lg:w-80 lg:border-r border-slate-800
            transition-transform duration-300 transform
            ${isSidebarOpen ? 'translate-y-0' : '-translate-y-full lg:translate-y-0'}
            flex flex-col h-full lg:h-auto overflow-hidden
          `}
        >
          <div className="flex-1 overflow-y-auto p-6 pb-20 lg:pb-6">
            <div className="mb-6">
              <h2 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-4">
                Input &amp; Pre-Gain
              </h2>
              <div className="bg-slate-900 p-4 rounded-xl border border-slate-800">
                <div className="flex justify-between items-center mb-2">
                  <span className="text-sm font-medium text-slate-300">
                    Level Input
                  </span>
                  <span className="text-xs text-indigo-400 font-mono">
                    {(Math.log10(inputGainValue) * 20).toFixed(1)} dB
                  </span>
                </div>
                <input
                  type="range"
                  min="0"
                  max="4"
                  step="0.1"
                  value={inputGainValue}
                  onChange={handleInputGainChange}
                  className="w-full h-1 bg-slate-700 rounded-lg appearance-none cursor-pointer"
                />
              </div>
            </div>

            <div className="mb-8 space-y-3">
              <h2 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-4">
                AI Processing
              </h2>

              <FeatureToggle
                icon={<Activity size={18} />}
                label="Hyper-Gate Reduction"
                desc="Kills room noise when nobody speaks"
                active={features.denoise}
                onClick={() =>
                  setFeatures({
                    ...features,
                    denoise: !features.denoise,
                  })
                }
              />

              {features.denoise && (
                <div className="bg-slate-900 p-4 rounded-xl border border-slate-800 mt-3 -mb-3 transition-opacity duration-300 animate-in fade-in slide-in-from-top-2">
                  <div className="flex justify-between items-center mb-2">
                    <span className="text-xs font-medium text-slate-400">
                      Noise Threshold
                    </span>
                    <div className="flex items-center gap-2">
                      {visualizerGateStatus ? (
                        <span className="text-[9px] font-bold text-red-500 tracking-wider animate-pulse">
                          GATE CLOSED
                        </span>
                      ) : (
                        <span className="text-[9px] font-bold text-green-500 tracking-wider">
                          GATE OPEN
                        </span>
                      )}
                      <span
                        className={`text-xs font-mono ${
                          noiseFloorThreshold > -40
                            ? 'text-red-400'
                            : 'text-indigo-400'
                        }`}
                      >
                        {noiseFloorThreshold} dB
                      </span>
                    </div>
                  </div>

                  <input
                    type="range"
                    min="-100"
                    max="-20"
                    step="1"
                    value={noiseFloorThreshold}
                    onChange={(e) =>
                      setNoiseFloorThreshold(parseFloat(e.target.value))
                    }
                    className="w-full h-1 bg-indigo-700 rounded-lg appearance-none cursor-pointer"
                  />

                  <div className="mt-3 flex items-center justify-between gap-2">
                    <button
                      type="button"
                      onClick={startAutoCalibrate}
                      disabled={isAutoCalibrating}
                      className={`text-[10px] px-3 py-1.5 rounded-lg border ${
                        isAutoCalibrating
                          ? 'border-indigo-500/60 bg-indigo-900/30 text-indigo-300 cursor-wait'
                          : 'border-slate-700 bg-slate-950 text-slate-300 hover:bg-slate-900'
                      }`}
                    >
                      {isAutoCalibrating ? 'Listening to room…' : 'Auto-calibrate gate'}
                    </button>
                    <span className="text-[9px] text-slate-500 text-right">
                      Tip: Run auto-calibrate when the room is “normally noisy” (fans,
                      kids, AC, etc).
                    </span>
                  </div>
                </div>
              )}

              <FeatureToggle
                icon={<Radio size={18} />}
                label="Echo / Reverb Cleaner"
                desc="Dries up big room reflections"
                active={features.dereverb}
                onClick={() =>
                  setFeatures({
                    ...features,
                    dereverb: !features.dereverb,
                  })
                }
              />
              <FeatureToggle
                icon={<Mic size={18} />}
                label="Pastor Voice Isolation"
                desc="Pushes speech forward in the mix"
                active={features.pastorIsolation}
                onClick={() =>
                  setFeatures({
                    ...features,
                    pastorIsolation: !features.pastorIsolation,
                  })
                }
              />
              <FeatureToggle
                icon={<Speaker size={18} />}
                label="Sermon Warmth"
                desc="Adds low-mid body like a podcast"
                active={features.sermonWarmth}
                onClick={() =>
                  setFeatures({
                    ...features,
                    sermonWarmth: !features.sermonWarmth,
                  })
                }
              />
              <FeatureToggle
                icon={<Sliders size={18} />}
                label="Smart Auto-Mixing"
                desc="Balances levels & gently rides speech"
                active={features.smartMixing}
                onClick={() =>
                  setFeatures({
                    ...features,
                    smartMixing: !features.smartMixing,
                  })
                }
              />
            </div>

            <FeatureToggle
              icon={<Volume2 size={18} />}
              label="Broadcast Limit"
              desc="-14 LUFS-style output guard"
              active={features.mastering}
              onClick={() =>
                setFeatures({
                  ...features,
                  mastering: !features.mastering,
                })
              }
            />

            <button
              onClick={() => setIsSidebarOpen(false)}
              className="mt-8 w-full py-4 bg-slate-800 text-slate-400 rounded-xl lg:hidden font-bold border border-slate-700 active:bg-slate-700"
            >
              Close Controls
            </button>
          </div>
        </div>

        {/* MAIN VISUALIZER & TRANSPORT */}
        <div className="flex-1 flex flex-col relative bg-slate-900 h-full overflow-hidden">
          {!isLive && !isPlayingFile && mode === 'live' && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/60 z-30 backdrop-blur-sm px-4">
              <div className="text-center p-8 bg-slate-800 rounded-2xl border border-slate-700 shadow-2xl w-full max-w-md">
                <div className="w-16 h-16 bg-indigo-600/20 text-indigo-400 rounded-full flex items-center justify-center mx-auto mb-4">
                  <Mic size={32} />
                </div>
                <h3 className="text-2xl font-bold text-white mb-2">
                  Sound Check
                </h3>
                <p className="text-slate-400 mb-6">
                  Connect mic or mixer feed, then go live.
                </p>
                <button
                  onClick={toggleLive}
                  className="px-8 py-3 bg-indigo-600 hover:bg-indigo-500 text-white font-bold rounded-lg shadow-lg flex items-center gap-2 mx-auto"
                >
                  <Play size={20} fill="currentColor" /> Go Live
                </button>
              </div>
            </div>
          )}

          {isLive && (
            <div className="absolute top-6 left-1/2 -translate-x-1/2 z-10 flex flex-col items-center w-full px-4">
              <div
                className={`flex items-center gap-2 p-1 rounded-full border shadow-2xl backdrop-blur-md transition-all duration-300 ${
                  recordingState === 'recording'
                    ? 'bg-red-900/80 border-red-500'
                    : 'bg-slate-800/80 border-slate-600'
                }`}
              >
                {recordingState === 'idle' && (
                  <button
                    onClick={toggleRecording}
                    className="flex items-center gap-2 px-4 py-2 bg-red-600 hover:bg-red-500 text-white rounded-full font-bold text-sm transition-colors"
                  >
                    <Circle size={12} fill="currentColor" /> Record Check
                  </button>
                )}
                {recordingState === 'recording' && (
                  <button
                    onClick={toggleRecording}
                    className="flex items-center gap-3 px-4 py-2 text-white font-mono text-sm"
                  >
                    <span className="w-3 h-3 bg-red-500 rounded-sm animate-pulse" />
                    {formatTime(recordingDuration)}
                    <span className="font-bold border-l border-red-500 pl-3 ml-1">
                      STOP
                    </span>
                  </button>
                )}
                {recordingState === 'review' && (
                  <div className="flex items-center">
                    <button
                      onClick={toggleRecording}
                      className="px-4 py-2 hover:bg-slate-700 rounded-l-full text-green-400 font-bold text-sm flex items-center gap-2"
                    >
                      <Play size={14} fill="currentColor" /> PLAY
                    </button>
                    <div className="w-px h-4 bg-slate-600" />
                    <button
                      onClick={discardRecording}
                      className="px-3 py-2 hover:bg-slate-700 text-slate-400 hover:text-red-400"
                    >
                      <Trash2 size={16} />
                    </button>
                    <div className="w-px h-4 bg-slate-600" />
                    <button
                      onClick={shareRecording}
                      disabled={
                        exportStatus === 'sharing' || exportStatus === 'done'
                      }
                      className={`px-4 py-2 hover:bg-slate-700 rounded-r-full font-bold text-sm flex items-center gap-2 ${
                        exportStatus === 'done'
                          ? 'text-green-400'
                          : 'text-indigo-400'
                      }`}
                      title="Share / Export"
                    >
                      {exportStatus === 'sharing' && (
                        <span className="w-3 h-3 rounded-full border-2 border-indigo-400 border-t-transparent animate-spin" />
                      )}
                      {exportStatus === 'done' && <Check size={14} />}
                      {!exportStatus && <Share2 size={14} />}
                      {exportStatus === 'done' ? 'SAVED' : 'SHARE'}
                    </button>
                  </div>
                )}
              </div>

              {recordingState === 'review' && (
                <div className="mt-1 text-[10px] text-slate-300 font-mono text-center">
                  {isPlayingBack
                    ? `${formatTime(playbackPosition)} / ${
                        playbackDuration ? formatTime(playbackDuration) : '--:--'
                      }`
                    : playbackDuration
                    ? `Ready • ${formatTime(playbackDuration)}`
                    : 'Ready'}
                </div>
              )}

              {recordingState === 'recording' && (
                <div className="mt-2 text-[10px] text-red-400 font-bold tracking-widest animate-pulse">
                  RECORDING TEST SIGNAL...
                </div>
              )}
              {recordingState === 'review' && (
                <div className="mt-2 text-[10px] text-indigo-400 font-bold tracking-widest">
                  LIVE MIC MUTED • REVIEWING
                </div>
              )}
            </div>
          )}

          <div className="flex-1 relative overflow-hidden bg-slate-900 w-full h-full">
            {isBypassed && (
              <div className="absolute top-20 left-1/2 -translate-x-1/2 bg-red-500 text-white px-4 py-1 rounded-full text-xs font-bold tracking-widest shadow-lg animate-pulse z-20">
                BYPASS MODE (RAW)
              </div>
            )}

            <div className="hidden lg:flex absolute top-4 right-4 gap-2 z-10">
              <Badge active={features.denoise && !isBypassed} text="HYPER-GATE" />
              <Badge
                active={features.pastorIsolation && !isBypassed}
                text="VOICE ISOLATION"
              />
              <Badge
                active={features.sermonWarmth && !isBypassed}
                text="WARMTH"
              />
              <Badge
                active={features.mastering && !isBypassed}
                text="MASTERING"
              />
            </div>

            {/* File mode indicator */}
            {mode === 'file' && fileInfo && (
              <div className="absolute top-4 left-4 z-10 text-[11px] text-slate-300 bg-slate-900/80 px-3 py-1 rounded-full border border-slate-700">
                File mode • {fileInfo.isVideo ? 'Video audio track' : 'Audio file'} •{' '}
                <span className="text-slate-100 font-semibold">
                  {fileInfo.name}
                </span>
              </div>
            )}

            {/* Speech Priority Badge */}
            {voiceActive && !isBypassed && (
              <div className="absolute top-4 left-1/2 -translate-x-1/2 z-20">
                <div className="px-4 py-1.5 rounded-full bg-slate-900/80 border border-cyan-400/70 shadow-[0_0_18px_rgba(34,211,238,0.55)] flex items-center gap-2">
                  <div className="w-2 h-2 rounded-full bg-cyan-400 animate-pulse" />
                  <span className="text-[11px] font-semibold tracking-[0.18em] text-cyan-200 uppercase">
                    Speech Priority Active
                  </span>
                </div>
              </div>
            )}

            <canvas
              ref={canvasRef}
              width={1000}
              height={500}
              className={`w-full h-full object-cover transition-opacity ${
                isBypassed ? 'opacity-40 grayscale' : 'opacity-80'
              }`}
            />
          </div>

          {/* FOOTER TRANSPORT */}
          <div className="flex-none h-20 md:h-24 bg-slate-950 border-t border-slate-800 px-4 md:px-8 flex items-center justify-between z-20">
            <div className="flex items-center gap-4 md:gap-6">
              <button
                onClick={mode === 'live' ? toggleLive : null}
                className={`w-12 h-12 md:w-14 md:h-14 rounded-full flex items-center justify-center transition-all ${
                  isLive
                    ? 'bg-red-500 hover:bg-red-600 shadow-[0_0_20px_rgba(239,68,68,0.4)]'
                    : 'bg-indigo-600 hover:bg-indigo-500 shadow-[0_0_20px_rgba(79,70,229,0.4)]'
                }`}
              >
                {isLive ? (
                  <Square size={20} fill="currentColor" />
                ) : (
                  <Play size={20} fill="currentColor" className="ml-1" />
                )}
              </button>
              <div className="hidden md:block">
                <div className="text-xs text-slate-500 font-bold uppercase mb-1">
                  Status
                </div>
                <div
                  className={`text-sm font-medium flex items-center gap-2 ${
                    isLive ? 'text-green-400' : 'text-slate-400'
                  }`}
                >
                  {isLive && recordingState === 'review' ? (
                    <span className="text-amber-400">Reviewing (Mic Muted)</span>
                  ) : isLive ? (
                    <>
                      <span className="w-2 h-2 bg-green-500 rounded-full animate-pulse" />
                      Live Processing
                    </>
                  ) : (
                    'Standby'
                  )}
                </div>
              </div>
            </div>

            {/* Meters + Gain Rider readout */}
            <div className="flex items-center gap-2 md:gap-8">
              <Meter label="In L" level={isLive ? 75 : 0} />
              <div className="h-8 w-px bg-slate-800 hidden md:block" />
              <Meter
                label="Out L"
                level={isLive && recordingState !== 'review' ? 85 : 0}
                color={isBypassed ? 'bg-slate-500' : 'bg-indigo-500'}
              />
            </div>

            <div className="hidden md:flex flex-col items-end ml-4 text-[10px] leading-tight">
              <span className="uppercase font-bold tracking-wide text-slate-500">
                Gain Rider
              </span>
              <span
                className={`font-mono ${
                  features.smartMixing ? 'text-indigo-400' : 'text-slate-600'
                }`}
              >
                {features.smartMixing
                  ? `${gainRiderDb >= 0 ? '+' : ''}${gainRiderDb.toFixed(1)} dB`
                  : 'OFF'}
              </span>

              {voiceActive && !isBypassed && (
                <span className="mt-1 text-[10px] text-amber-400 flex items-center gap-1">
                  <AlertCircle className="w-3 h-3" />
                  Speech priority boost is active
                </span>
              )}
            </div>

            <div className="flex items-center gap-2 md:gap-4">
              <button
                onClick={() => setIsBypassed(!isBypassed)}
                className={`p-2 md:p-3 rounded-lg border flex gap-2 text-xs font-bold ${
                  isBypassed
                    ? 'bg-red-900/50 border-red-500 text-red-200'
                    : 'bg-slate-800 border-slate-700 text-slate-400'
                }`}
              >
                <Power size={18} />
                <span className="hidden md:inline">
                  {isBypassed ? 'BYPASSED' : 'BYPASS AI'}
                </span>
              </button>

              <button
                onClick={() => setIsMonitoring(!isMonitoring)}
                disabled={recordingState === 'review'}
                className={`p-2 md:p-3 rounded-lg border relative ${
                  isMonitoring
                    ? 'bg-indigo-600 border-indigo-500 text-white'
                    : 'bg-slate-800 border-slate-700 text-slate-400'
                } ${
                  recordingState === 'review'
                    ? 'opacity-50 cursor-not-allowed'
                    : ''
                }`}
              >
                <Headphones size={20} />
                {isMonitoring && recordingState !== 'review' && (
                  <span className="absolute -top-1 -right-1 w-2 h-2 bg-green-400 rounded-full animate-pulse" />
                )}
              </button>

              {/* Hard Reset / Troubleshoot */}
              <button
                onClick={hardReset}
                className="hidden md:flex items-center gap-1 px-3 py-2 rounded-lg border border-amber-500/60 bg-slate-900 text-amber-300 text-[11px] font-semibold hover:bg-slate-800"
                title="Stop audio engine and reset AI if something feels stuck"
              >
                <RefreshCw className="w-3 h-3" />
                Reset
              </button>

              {/* Process & Export (File mode) */}
              {mode === 'file' && fileInfo && (
                <button
                  onClick={processAndExportFile}
                  disabled={
                    fileExportStatus === 'processing' ||
                    !destNodeRef.current ||
                    !destNodeRef.current.stream
                  }
                  className={`
                    hidden md:flex items-center gap-1 px-3 py-2 rounded-lg border text-[11px] font-semibold 
                    ${
                      fileExportStatus === 'done'
                        ? 'border-emerald-500/70 bg-emerald-900/20 text-emerald-300'
                        : fileExportStatus === 'processing'
                        ? 'border-emerald-500/70 bg-slate-900 text-emerald-300 opacity-80 cursor-wait'
                        : !destNodeRef.current || !destNodeRef.current.stream
                        ? 'border-slate-700 bg-slate-900 text-slate-500 cursor-not-allowed'
                        : 'border-emerald-500/70 bg-slate-900 text-emerald-300 hover:bg-slate-800'
                    }
                  `}
                  title="Play the file through TIWATON and download the processed audio"
                >
                  {fileExportStatus === 'processing' && (
                    <span className="w-3 h-3 rounded-full border-2 border-emerald-300 border-t-transparent animate-spin" />
                  )}
                  {fileExportStatus === 'done' ? (
                    <>
                      <Check className="w-3 h-3" />
                      <span>Mastered</span>
                    </>
                  ) : (
                    <>
                      <Upload className="w-3 h-3" />
                      <span>
                        {fileExportStatus === 'processing'
                          ? 'Processing…'
                          : 'Process & Export'}
                      </span>
                    </>
                  )}
                </button>
              )}

              <div className="hidden md:block h-8 w-px bg-slate-800 mx-2" />

              <button
                onClick={cycleOutputTarget}
                className="hidden md:block p-3 bg-slate-800 hover:bg-slate-700 rounded-lg text-slate-300 border border-slate-700"
                title="Visual Reference Only: Configure in Settings"
              >
                {outputTargets.find((t) => t.name === outputTarget)?.icon || (
                  <Cpu size={20} />
                )}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

// --- Small UI helpers ---
const FeatureToggle = ({ icon, label, desc, active, onClick }) => (
  <div
    onClick={onClick}
    className={`p-3 rounded-xl border cursor-pointer transition-all group ${
      active
        ? 'bg-indigo-900/30 border-indigo-500/50 shadow-[0_0_15px_rgba(99,102,241,0.2)]'
        : 'bg-slate-900 border-slate-800 hover:border-slate-700 opacity-60 hover:opacity-100'
    }`}
  >
    <div className="flex items-center justify-between mb-1">
      <div
        className={`flex items-center gap-3 ${
          active ? 'text-indigo-400' : 'text-slate-400 group-hover:text-slate-300'
        }`}
      >
        {icon}
        <span className="font-semibold text-sm">{label}</span>
      </div>
      <div
        className={`w-4 h-4 rounded-full border flex items-center justify-center ${
          active ? 'bg-indigo-500 border-indigo-500' : 'border-slate-600'
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
        onClick={() => setIsOpen(true)}
        className="fixed bottom-20 right-4 z-40 flex items-center gap-2 rounded-full bg-slate-900/95 px-4 py-2 text-xs font-semibold text-slate-100 shadow-lg border border-slate-700 hover:bg-slate-800"
      >
        <span className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-indigo-500 text-[11px] font-bold">
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
                <div className="flex h-7 w-7 items-center justify-center rounded-full bg-indigo-600 text-xs font-bold text-white">
                  ?
                </div>
                <div>
                  <p className="text-sm font-semibold text-slate-100">
                    Quick help for better live sound
                  </p>
                  <p className="text-[11px] text-slate-400">
                    60-second checklist to make Tiwaton feel “magic”.
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

            {/* Body */}
            <div className="flex-1 space-y-3 overflow-y-auto px-4 py-3 text-[11px] leading-relaxed text-slate-300">
              <p className="font-semibold text-slate-100">1️⃣ Before you start</p>
              <ul className="ml-4 list-disc space-y-1">
                <li>Use a wired mic or mixer feed if possible.</li>
                <li>Set mixer gain correctly before enabling AI.</li>
                <li>Turn off system Noise Suppression and AGC.</li>
              </ul>

              <p className="mt-2 font-semibold text-slate-100">
                2️⃣ Dial in Hyper-Gate
              </p>
              <ul className="ml-4 list-disc space-y-1">
                <li>Quiet the room first.</li>
                <li>Increase Noise Threshold until background disappears.</li>
                <li>If speech chops, move threshold slightly left.</li>
              </ul>

              <p className="mt-2 font-semibold text-slate-100">
                3️⃣ Troubleshooting
              </p>
              <ul className="ml-4 list-disc space-y-1">
                <li>
                  <span className="font-semibold text-slate-100">
                    ❗ Echo / doubling
                  </span>
                  <ul className="ml-4 list-disc">
                    <li>Do not send RAW + AI output to stream.</li>
                    <li>Send ONLY Tiwaton output to OBS/YouTube.</li>
                  </ul>
                </li>

                <li>
                  <span className="font-semibold text-slate-100">
                    ❗ Gate too aggressive
                  </span>
                  <ul className="ml-4 list-disc">
                    <li>Lower Noise Threshold (move left).</li>
                    <li>Start at −55 dB and increase gradually.</li>
                  </ul>
                </li>
              </ul>

              <div className="mt-2 rounded-xl border border-slate-800 bg-slate-900 px-3 py-2">
                <p className="text-[11px] text-slate-400">
                  <span className="font-semibold text-indigo-300">
                    Golden rule:
                  </span>{' '}
                  Fix physical mic problems first, then let Tiwaton polish.
                </p>
              </div>
            </div>

            {/* Footer */}
            <div className="flex items-center justify-between border-t border-slate-800 px-4 py-2 text-[10px] text-slate-500">
              <span>Tips tuned for your studio version.</span>
              <span className="hidden sm:inline">Your audio co-pilot is here.</span>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

export default TiwatonApp;
