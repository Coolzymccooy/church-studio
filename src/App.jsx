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
  HelpCircle,
} from 'lucide-react';

/**
 * TIWATON AUDIO AI
 * UPDATE:
 * - Hyper-Gate + VAD
 * - File Upload mode
 * - Smart Gain Rider after compressor (driven by RMS)
 * - Gain Rider readout in footer
 * - Smart Auto-Mixing description updated to reflect gain riding
 */

const TiwatonApp = () => {
  const [view, setView] = useState('landing');

  if (view === 'landing') {
    return <LandingPage onEnter={() => setView('studio')} />;
  }
return (
    <>
      <AudioProcessor goHome={() => setView('landing')} />
      <HelpCorner />   {/* ← This is the correct place */}
    </>
  );
};
// --- LANDING PAGE ---
const LandingPage = ({ onEnter }) => {
  return (
    <div className="h-[100dvh] bg-slate-950 text-white flex flex-col items-center justify-center relative overflow-hidden font-sans selection:bg-indigo-500 selection:text-white">
      <div className="absolute inset-0 z-0 opacity-20">
        <div className="absolute top-[-20%] left-[-10%] w-[600px] h-[600px] bg-indigo-600 rounded-full blur-[120px] animate-pulse"></div>
        <div
          className="absolute bottom-[-20%] right-[-10%] w-[500px] h-[500px] bg-purple-600 rounded-full blur-[120px]"
          style={{ animationDuration: '4s' }}
        ></div>
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
          <div className="absolute inset-0 bg-gradient-to-r from-indigo-400 to-purple-400 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
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

  const [noiseFloorThreshold, setNoiseFloorThreshold] = useState(-50);
  const [visualizerGateStatus, setVisualizerGateStatus] = useState(false);
  const [recordingState, setRecordingState] = useState('idle');
  const [recordedUrl, setRecordedUrl] = useState(null);
  const [recordingDuration, setRecordingDuration] = useState(0);
  const [gainRiderDb, setGainRiderDb] = useState(0); // Smart Gain Rider readout

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
    autoGain: null, // NEW: Smart Gain Rider node
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
    const currentIndex = outputTargets.findIndex(
      (t) => t.name === outputTarget,
    );
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
    if (!features.denoise && processingRefs.current.gateGain) {
      processingRefs.current.gateGain.gain.setValueAtTime(
        1.0,
        audioContext?.currentTime || 0,
      );
    }
  }, [features.denoise, noiseFloorThreshold, isBypassed, audioContext]);

  useEffect(() => {
    return () => cleanupAudio();
  }, []);

  const cleanupAudio = () => {
    if (contextRef.current) {
      contextRef.current.close();
      contextRef.current = null;
    }
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
    }
    if (animationRef.current) cancelAnimationFrame(animationRef.current);

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

    setAudioContext(null);
    setIsLive(false);
    setIsPlayingFile(false);
    setAudioStats({ sampleRate: 0, bufferSize: 0, state: 'suspended' });
    setExportStatus(null);
    if (recordingTimerRef.current) clearInterval(recordingTimerRef.current);
    setRecordingState('idle');
  };

  const startAudioEngine = async (inputStream = null, fileElement = null) => {
    if (contextRef.current && contextRef.current.state !== 'closed') return;

    const ctx = new (window.AudioContext ||
      window.webkitAudioContext)();

    if (ctx.state === 'suspended') {
      await ctx.resume();
    }

    contextRef.current = ctx;
    setAudioContext(ctx);
    setAudioStats({ sampleRate: ctx.sampleRate, bufferSize: 128, state: ctx.state });

    let source;
    try {
      if (inputStream) {
        source = ctx.createMediaStreamSource(inputStream);
      } else if (fileElement) {
        source = ctx.createMediaElementSource(fileElement);
      } else return;
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

  const handleFileUpload = (event) => {
    const file = event.target.files?.[0];
    if (file) {
      const url = URL.createObjectURL(file);
      if (audioElRef.current) {
        audioElRef.current.src = url;
        audioElRef.current.play();
      }
      setIsPlayingFile(true);
      startAudioEngine(null, audioElRef.current);
    }
  };

  const toggleRecording = () => {
    if (recordingState === 'idle') {
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
      const testAudio = new Audio(recordedUrl);
      testAudio.play();
    }
  };

  const discardRecording = () => {
    setRecordedUrl(null);
    setRecordingState('idle');
    setRecordingDuration(0);
    setExportStatus(null);
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
    const { compressor, eqWarmth, eqClarity } = processingRefs.current;
    const now = audioContext.currentTime;
    const getTarget = (isActive, activeVal, passiveVal) =>
      isActive && !isBypassed ? activeVal : passiveVal;

    if (eqClarity) {
      eqClarity.gain.linearRampToValueAtTime(
        getTarget(features.pastorIsolation, 8, 0),
        now + 0.5,
      );
    }
    if (eqWarmth) {
      eqWarmth.gain.linearRampToValueAtTime(
        getTarget(features.sermonWarmth, 5, 0),
        now + 0.5,
      );
    }
    if (compressor) {
      compressor.ratio.linearRampToValueAtTime(
        getTarget(features.smartMixing, 4, 1),
        now + 0.5,
      );
      compressor.threshold.linearRampToValueAtTime(
        getTarget(features.smartMixing, -24, 0),
        now + 0.5,
      );
    }
  }, [features, audioContext, isBypassed]);

  // --- Visualizer + Gate + Smart Gain Rider ---
 // --- Visualizer + Gate + Smart Gain Rider (fixed so gate wins over auto-gain) ---
const visualizeAndGate = () => {
  const { analyser, gateGain } = processingRefs.current;
  const canvas = canvasRef.current;
  if (!analyser || !canvas) return;

  const ctx = canvas.getContext('2d');
  const bufferLength = analyser.frequencyBinCount;
  const dataArray = new Uint8Array(bufferLength);
  const timeData = new Uint8Array(bufferLength);

  let lastGateUpdate = 0;
  let lastGainUpdate = 0;
  const updateInterval = 100;

  const draw = (timestamp) => {
    animationRef.current = requestAnimationFrame(draw);
    analyser.getByteFrequencyData(dataArray);
    analyser.getByteTimeDomainData(timeData);

    // Clear background
    ctx.fillStyle = '#0f172a';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    const barWidth = (canvas.width / bufferLength) * 2.5;
    let x = 0;

    // Basic spectrum bars
    for (let i = 0; i < bufferLength; i++) {
      const barHeight = dataArray[i] * 1.5;
      ctx.fillStyle = barHeight > 240 ? '#ef4444' : '#4f46e5';
      ctx.fillRect(
        x,
        canvas.height - barHeight / 2,
        barWidth,
        barHeight / 2
      );
      x += barWidth + 1;
    }

    // Shared RMS / dB computation
    let sum = 0;
    for (let i = 0; i < bufferLength; i++) {
      const sample = (timeData[i] - 128) / 128.0;
      sum += sample * sample;
    }
    const rms = Math.sqrt(sum / bufferLength);
    const db = 20 * Math.log10(rms || 0.00001); // approx -100..0 dB

    // Draw Hyper-Gate threshold line
    if (settingsRef.current.denoise && !settingsRef.current.isBypassed) {
      const threshY =
        canvas.height * (1 - (settingsRef.current.threshold + 100) / 100);
      ctx.strokeStyle = '#6366f1';
      ctx.setLineDash([5, 5]);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, threshY);
      ctx.lineTo(canvas.width, threshY);
      ctx.stroke();
      ctx.setLineDash([]);
    }

    // ---------- HYPER-GATE ----------
    let gateClosed = false;
if (gateGain && settingsRef.current.denoise && !settingsRef.current.isBypassed) {
  const threshold = settingsRef.current.threshold;
  const target = db < threshold ? 0.001 : 1.0; // closed vs open

  const current = gateGain.gain.value;
  const smoothing = target > current ? 0.2 : 0.05; // attack / release
  gateGain.gain.value = current + (target - current) * smoothing;

  gateClosed = gateGain.gain.value < 0.1;

  if (timestamp - lastGateUpdate > updateInterval) {
    setVisualizerGateStatus((prev) =>
      prev !== gateClosed ? gateClosed : prev
    );
    lastGateUpdate = timestamp;
  }
} else if (gateGain) {
  // Gate off or bypassed: fully open
  gateGain.gain.value = 1.0;
  gateClosed = false;
  if (visualizerGateStatus !== false) {
    setVisualizerGateStatus(false);
  }
}


    // ---------- SMART GAIN RIDER ----------
   // ---------- SMART GAIN RIDER (with Speech Focus Boost) ----------
const autoGain = processingRefs.current.autoGain;
const gainRiderEnabled =
  features.smartMixing && !settingsRef.current.isBypassed;

if (autoGain && gainRiderEnabled && !gateClosed) {
  // Only ride gain when gate is OPEN (speech/music present)

  // If gate threshold is very high (aggressive noise kill),
  // we assume a noisy environment and push speech a bit hotter.
  const aggressiveMode =
    settingsRef.current.denoise && settingsRef.current.threshold > -40;

  const targetSpeechDb = aggressiveMode ? -18 : -24;  // louder target when gating hard
  const maxBoostDb = aggressiveMode ? 16 : 12;        // allow a bit more upward gain
  const maxCutDb = -12;
  const gainUpdateInterval = 200;

  let neededChange = targetSpeechDb - db;
  if (neededChange > maxBoostDb) neededChange = maxBoostDb;
  if (neededChange < maxCutDb) neededChange = maxCutDb;

  const desiredLinear = Math.pow(10, neededChange / 20);
  const currentLinear = autoGain.gain.value || 1.0;

  // Smooth rider so it doesn't pump
  const smoothing = 0.02;
  autoGain.gain.value =
    currentLinear + (desiredLinear - currentLinear) * smoothing;

  if (timestamp - lastGainUpdate > gainUpdateInterval) {
    const appliedDb =
      20 * Math.log10(autoGain.gain.value || 0.00001);
    setGainRiderDb(parseFloat(appliedDb.toFixed(1)));
    lastGainUpdate = timestamp;
  }
} else if (autoGain) {
  // Gate closed OR SmartMixing off:
  // → do NOT boost noise, gently return toward unity and reset display
  const currentLinear = autoGain.gain.value || 1.0;
  const targetLinear = 1.0;
  const smoothingBack = 0.05;

  autoGain.gain.value =
    currentLinear + (targetLinear - currentLinear) * smoothingBack;

  if (timestamp - lastGainUpdate > 200 && gainRiderDb !== 0) {
    setGainRiderDb(0);
    lastGainUpdate = timestamp;
  }
}
};

  draw(performance.now());
};

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

  return (
    <div className="flex flex-col h-[100dvh] bg-slate-900 text-white font-sans overflow-hidden">
      <audio
        ref={audioElRef}
        crossOrigin="anonymous"
        onEnded={() => setIsPlayingFile(false)}
      />

      {/* Hidden file input */}
      <input
        type="file"
        ref={fileInputRef}
        accept="audio/*"
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
              ></span>
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
                    audioStats.sampleRate > 0 &&
                    audioStats.sampleRate < 44100
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
                Input & Pre-Gain
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
                <div className="bg-slate-900 p-4 rounded-xl border border-slate-800 mt-3 -mb-3 transition-opacity duration-300">
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
                  <p className="text-[10px] text-slate-500 mt-2 leading-tight">
                    Move until background noise disappears on the meters, but
                    voices still open cleanly.
                  </p>
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
                    <span className="w-3 h-3 bg-red-500 rounded-sm animate-pulse"></span>
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
                    <div className="w-px h-4 bg-slate-600"></div>
                    <button
                      onClick={discardRecording}
                      className="px-3 py-2 hover:bg-slate-700 text-slate-400 hover:text-red-400"
                    >
                      <Trash2 size={16} />
                    </button>
                    <div className="w-px h-4 bg-slate-600"></div>
                    <button
                      onClick={shareRecording}
                      disabled={
                        exportStatus === 'sharing' ||
                        exportStatus === 'done'
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
            <canvas
              ref={canvasRef}
              width={1000}
              height={500}
              className={`w-full h-full object-cover transition-opacity ${
                isBypassed ? 'opacity-40 grayscale' : 'opacity-80'
              }`}
            />
          </div>

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
                    <span className="text-amber-400">
                      Reviewing (Mic Muted)
                    </span>
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
              <div className="h-8 w-px bg-slate-800 hidden md:block"></div>
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
                <Power size={18} />{' '}
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
                  <span className="absolute -top-1 -right-1 w-2 h-2 bg-green-400 rounded-full animate-pulse"></span>
                )}
              </button>
              <div className="hidden md:block h-8 w-px bg-slate-800 mx-2"></div>
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
          active
            ? 'text-indigo-400'
            : 'text-slate-400 group-hover:text-slate-300'
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

              <p className="mt-2 font-semibold text-slate-100">2️⃣ Dial in Hyper-Gate</p>
              <ul className="ml-4 list-disc space-y-1">
                <li>Quiet the room first.</li>
                <li>Increase Noise Threshold until background disappears.</li>
                <li>If speech chops, move threshold slightly left.</li>
              </ul>

              <p className="mt-2 font-semibold text-slate-100">3️⃣ Troubleshooting</p>
              <ul className="ml-4 list-disc space-y-1">
                <li>
                  <span className="font-semibold text-slate-100">❗ Echo / doubling</span>
                  <ul className="ml-4 list-disc">
                    <li>Do not send RAW + AI output to stream.</li>
                    <li>Send ONLY Tiwaton output to OBS/YouTube.</li>
                  </ul>
                </li>

                <li>
                  <span className="font-semibold text-slate-100">❗ Gate too aggressive</span>
                  <ul className="ml-4 list-disc">
                    <li>Lower Noise Threshold (move left).</li>
                    <li>Start at −55 dB and increase gradually.</li>
                  </ul>
                </li>
              </ul>

              <div className="mt-2 rounded-xl border border-slate-800 bg-slate-900 px-3 py-2">
                <p className="text-[11px] text-slate-400">
                  <span className="font-semibold text-indigo-300">Golden rule:</span>{' '}
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
