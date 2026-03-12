import { useEffect, useRef, useState } from 'react';
import {
  Mic, Waves, Radio, Sliders, Volume2, Zap, Shield, Download,
  ChevronRight, Star, Activity, Layers, AudioLines, Cpu, Globe,
  ArrowRight, Check, Github,
} from 'lucide-react';

const GITHUB = 'https://github.com/Coolzymccooy/church-studio';
const RELEASES = `${GITHUB}/releases/latest`;

/* ── Animated spectrum bars (hero visual) ─────────────────────────────── */
function SpectrumViz() {
  const canvasRef = useRef(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const bars = Array.from({ length: 64 }, (_, i) => ({
      h: 10 + Math.random() * 60,
      speed: 0.4 + Math.random() * 1.2,
      phase: Math.random() * Math.PI * 2,
      base: 8 + i * 0.6,
    }));
    let frame = 0;
    let raf;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      frame++;
      const w = canvas.width, h = canvas.height;
      ctx.clearRect(0, 0, w, h);
      const barW = w / bars.length - 1;
      bars.forEach((bar, i) => {
        const t = frame * 0.012 * bar.speed + bar.phase;
        const bh = bar.base + Math.abs(Math.sin(t) * bar.h + Math.cos(t * 0.7) * bar.h * 0.4);
        const x = i * (barW + 1);
        // Voice band (20-45) brighter
        const inVoice = i >= 20 && i <= 45;
        const alpha = inVoice ? 0.85 : 0.4;
        const grad = ctx.createLinearGradient(0, h - bh, 0, h);
        grad.addColorStop(0, `rgba(245,158,11,${alpha})`);
        grad.addColorStop(1, `rgba(245,158,11,0.1)`);
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.roundRect(x, h - bh, barW, bh, [2, 2, 0, 0]);
        ctx.fill();
      });
    };
    draw();
    return () => cancelAnimationFrame(raf);
  }, []);
  return <canvas ref={canvasRef} width={640} height={120} className="w-full opacity-80" />;
}

/* ── Animated waveform line ──────────────────────────────────────────────*/
function WaveformLine({ color = '#F59E0B', opacity = 0.6 }) {
  const canvasRef = useRef(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    let frame = 0, raf;
    const draw = () => {
      raf = requestAnimationFrame(draw);
      frame++;
      const w = canvas.width, h = canvas.height;
      ctx.clearRect(0, 0, w, h);
      ctx.strokeStyle = color;
      ctx.globalAlpha = opacity;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      for (let x = 0; x <= w; x++) {
        const t = (x / w) * Math.PI * 8 + frame * 0.03;
        const amp = (h / 2) * 0.35 * (1 + Math.sin(frame * 0.015 + x * 0.02));
        const y = h / 2 + Math.sin(t) * amp * 0.7 + Math.sin(t * 2.3) * amp * 0.3;
        x === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;
    };
    draw();
    return () => cancelAnimationFrame(raf);
  }, [color, opacity]);
  return <canvas ref={canvasRef} width={800} height={40} className="w-full" />;
}

/* ── Feature card ────────────────────────────────────────────────────────*/
function FeatureCard({ icon, label, desc, tag }) {
  return (
    <div className="rounded-xl border border-slate-800 p-4 hover:border-amber-500/30 transition-all group"
      style={{ background: 'linear-gradient(135deg,#080E1F 0%,#050A1C 100%)' }}>
      <div className="flex items-start justify-between mb-2">
        <div className="p-2 rounded-lg" style={{ background: 'rgba(245,158,11,0.1)' }}>
          <span className="text-amber-400">{icon}</span>
        </div>
        {tag && (
          <span className="text-[8px] font-bold px-1.5 py-0.5 rounded border border-amber-500/30 text-amber-400"
            style={{ background: 'rgba(245,158,11,0.08)' }}>{tag}</span>
        )}
      </div>
      <h3 className="text-[11px] font-bold text-white mt-2 mb-1">{label}</h3>
      <p className="text-[10px] text-slate-500 leading-relaxed">{desc}</p>
    </div>
  );
}

/* ── DSP chain node ──────────────────────────────────────────────────────*/
function ChainNode({ label, sub, color = '#F59E0B' }) {
  return (
    <div className="flex flex-col items-center gap-1">
      <div className="px-3 py-2 rounded-lg border text-center min-w-[80px]"
        style={{ borderColor: `${color}30`, background: `${color}10` }}>
        <div className="text-[9px] font-bold" style={{ color }}>{label}</div>
        {sub && <div className="text-[8px] text-slate-600 mt-0.5">{sub}</div>}
      </div>
    </div>
  );
}
function Arrow() {
  return <div className="text-slate-700 text-[10px] font-bold self-center">›</div>;
}

/* ── Stat card ───────────────────────────────────────────────────────────*/
function Stat({ value, label }) {
  return (
    <div className="text-center">
      <div className="text-lg font-black text-amber-400 font-mono">{value}</div>
      <div className="text-[9px] text-slate-600 mt-0.5">{label}</div>
    </div>
  );
}

/* ── Main landing page ───────────────────────────────────────────────────*/
export default function LandingPage({ onLaunch }) {
  const [os] = useState(() => {
    const ua = navigator.platform.toLowerCase();
    if (ua.includes('mac')) return 'mac';
    if (ua.includes('linux')) return 'linux';
    return 'win';
  });

  const dlLabel = os === 'mac' ? 'Download for macOS' : os === 'linux' ? 'Download for Linux' : 'Download for Windows';

  return (
    <div className="min-h-screen text-white overflow-x-hidden" style={{ background: '#020617', fontFamily: "system-ui,-apple-system,'Segoe UI',sans-serif" }}>

      {/* ── NAV ─────────────────────────────────────────────────────────── */}
      <nav className="sticky top-0 z-50 border-b border-slate-800/60 backdrop-blur-md"
        style={{ background: 'rgba(2,6,23,0.85)' }}>
        <div className="max-w-5xl mx-auto px-6 h-12 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="w-5 h-5 rounded flex items-center justify-center" style={{ background: 'rgba(245,158,11,0.15)', border: '1px solid rgba(245,158,11,0.3)' }}>
              <Mic size={10} className="text-amber-400" />
            </div>
            <span className="text-[11px] font-black tracking-[0.15em] text-white uppercase">TIWATON</span>
            <span className="text-[9px] text-slate-600 font-mono border border-slate-800 px-1.5 py-0.5 rounded ml-1">v1.4.2</span>
          </div>
          <div className="flex items-center gap-3">
            <a href={GITHUB} target="_blank" rel="noreferrer"
              className="text-slate-500 hover:text-white transition-colors">
              <Github size={14} />
            </a>
            <button onClick={onLaunch}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[10px] font-bold text-black transition-all hover:opacity-90"
              style={{ background: 'linear-gradient(135deg,#F59E0B,#D97706)' }}>
              Launch Studio <ArrowRight size={10} />
            </button>
          </div>
        </div>
      </nav>

      {/* ── HERO ────────────────────────────────────────────────────────── */}
      <section className="relative max-w-5xl mx-auto px-6 pt-16 pb-12 overflow-hidden">
        {/* Background glow */}
        <div className="absolute inset-0 pointer-events-none">
          <div className="absolute top-8 left-1/2 -translate-x-1/2 w-[500px] h-[300px] rounded-full blur-[120px] opacity-12"
            style={{ background: 'radial-gradient(ellipse,#F59E0B 0%,transparent 70%)' }} />
        </div>

        <div className="relative text-center mb-8">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full border border-amber-500/20 text-amber-400 text-[9px] font-bold tracking-[0.1em] mb-5"
            style={{ background: 'rgba(245,158,11,0.06)' }}>
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
            AI-POWERED AUDIO — OPEN SOURCE
          </div>

          <h1 className="text-3xl font-black text-white leading-tight tracking-tight mb-3">
            Professional Church Audio<br />
            <span style={{ background: 'linear-gradient(90deg,#F59E0B,#FCD34D)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' }}>
              Powered by AI
            </span>
          </h1>

          <p className="text-[13px] text-slate-400 max-w-lg mx-auto leading-relaxed mb-6">
            Studio-grade voice processing, spectral dereverberation, and broadcast-quality
            mastering — purpose-built for live church production.
          </p>

          <div className="flex items-center justify-center gap-3 flex-wrap">
            <button onClick={onLaunch}
              className="flex items-center gap-2 px-5 py-2.5 rounded-xl text-[11px] font-bold text-black transition-all hover:scale-105 active:scale-95"
              style={{ background: 'linear-gradient(135deg,#F59E0B,#D97706)', boxShadow: '0 0 20px rgba(245,158,11,0.3)' }}>
              <Zap size={12} /> Launch Web Studio
            </button>
            <a href={RELEASES} target="_blank" rel="noreferrer"
              className="flex items-center gap-2 px-5 py-2.5 rounded-xl text-[11px] font-bold text-white border border-slate-700 hover:border-amber-500/40 transition-all hover:scale-105"
              style={{ background: 'rgba(255,255,255,0.04)' }}>
              <Download size={12} /> {dlLabel}
            </a>
          </div>
        </div>

        {/* Spectrum viz */}
        <div className="relative rounded-2xl border border-slate-800 overflow-hidden p-4"
          style={{ background: 'linear-gradient(180deg,#080E1F 0%,#020617 100%)' }}>
          <div className="flex items-center gap-1.5 mb-3 opacity-60">
            <div className="w-2.5 h-2.5 rounded-full bg-red-500/70" />
            <div className="w-2.5 h-2.5 rounded-full bg-amber-500/70" />
            <div className="w-2.5 h-2.5 rounded-full bg-green-500/70" />
            <span className="text-[8px] text-slate-600 ml-2 font-mono">TIWATON AI Studio — Live Spectrum</span>
          </div>
          <SpectrumViz />
          <WaveformLine />
          <div className="absolute top-4 right-4 flex items-center gap-1.5">
            <div className="w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse" />
            <span className="text-[8px] text-green-400 font-mono font-bold">LIVE</span>
          </div>
          {/* Fake DSP labels */}
          <div className="flex justify-between mt-2 px-1">
            {['80Hz HPF','RNNoise','HyperGate','4-Band Comp','LUFS Meter'].map(l => (
              <span key={l} className="text-[7px] font-mono text-slate-700">{l}</span>
            ))}
          </div>
        </div>

        {/* Stats row */}
        <div className="grid grid-cols-4 gap-4 mt-6 py-4 border-y border-slate-800/50">
          <Stat value="27+" label="DSP Processors" />
          <Stat value="<1ms" label="Gate Latency" />
          <Stat value="BS.1770" label="LUFS Standard" />
          <Stat value="100%" label="Open Source" />
        </div>
      </section>

      {/* ── CORE FEATURES ───────────────────────────────────────────────── */}
      <section className="max-w-5xl mx-auto px-6 py-10">
        <div className="text-center mb-6">
          <p className="text-[8px] font-bold tracking-[0.2em] text-amber-400 uppercase mb-1">Processing Stack</p>
          <h2 className="text-lg font-black text-white">Studio-Grade DSP Chain</h2>
          <p className="text-[11px] text-slate-500 mt-1">Every processor runs in real-time, AudioWorklet-native</p>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          <FeatureCard icon={<Zap size={14}/>} tag="NEW" label="Lookahead Gate" desc="20ms ring-buffer pre-opens the gate before speech arrives — no clipped first syllables." />
          <FeatureCard icon={<Activity size={14}/>} tag="NEW" label="Dynamic De-esser" desc="Sidechain bandpass detects sibilance per frame. Compresses only during 'ss/sh' events." />
          <FeatureCard icon={<Sliders size={14}/>} tag="NEW" label="4-Band Compressor" desc="Independent sub / low-mid / mid / air compression. Speech band gets tightest ratio." />
          <FeatureCard icon={<Radio size={14}/>} tag="NEW" label="Spectral Dereverb" desc="Overlap-add FFT minimum-statistics reverb tail subtraction. True dereverberation." />
          <FeatureCard icon={<Layers size={14}/>} tag="NEW" label="True LUFS Meter" desc="ITU-R BS.1770-4 K-weighted loudness. Momentary, short-term, and integrated LUFS." />
          <FeatureCard icon={<Cpu size={14}/>} label="HyperGate AI" desc="512-pt FFT voice scoring: formant peaks, spectral flatness, ZCR. Sub-1ms reaction time." />
          <FeatureCard icon={<Waves size={14}/>} label="RNNoise Neural" desc="Mozilla's deep-learning noise model running in WebAssembly for neural suppression." />
          <FeatureCard icon={<Mic size={14}/>} label="Pastor Isolation" desc="Boosts presence, tightens compressor, narrows de-esser — locks in on the pastor voice." />
        </div>
      </section>

      {/* ── DSP SIGNAL CHAIN ────────────────────────────────────────────── */}
      <section className="border-y border-slate-800/60 py-10" style={{ background: 'rgba(8,14,31,0.5)' }}>
        <div className="max-w-5xl mx-auto px-6">
          <div className="text-center mb-6">
            <p className="text-[8px] font-bold tracking-[0.2em] text-amber-400 uppercase mb-1">Architecture</p>
            <h2 className="text-lg font-black text-white">Full Signal Chain</h2>
          </div>
          <div className="overflow-x-auto pb-2">
            <div className="flex items-center gap-1 min-w-max mx-auto justify-center flex-wrap gap-y-3">
              <ChainNode label="MIC IN" sub="Input Gain" color="#94A3B8" />
              <Arrow/>
              <ChainNode label="HPF" sub="80 Hz" />
              <Arrow/>
              <ChainNode label="RNNoise" sub="Neural WASM" />
              <Arrow/>
              <ChainNode label="Lookahead" sub="20ms gate" color="#00E676" />
              <Arrow/>
              <ChainNode label="HyperGate" sub="FFT voice" color="#00E676" />
              <Arrow/>
              <ChainNode label="Dyn De-ess" sub="Sidechain" />
              <Arrow/>
              <ChainNode label="4-Band" sub="Multiband" />
              <Arrow/>
              <ChainNode label="Compressor" sub="-24dB / 4:1" />
              <Arrow/>
              <ChainNode label="Dereverb" sub="Spectral" color="#8B5CF6" />
              <Arrow/>
              <ChainNode label="Polish EQ" sub="Air/Presence" />
              <Arrow/>
              <ChainNode label="Limiter" sub="-1dBFS / 20:1" color="#EF4444" />
              <Arrow/>
              <ChainNode label="LUFS Tap" sub="BS.1770-4" color="#00E676" />
              <Arrow/>
              <ChainNode label="OUT" sub="VB-CABLE / OBS" color="#94A3B8" />
            </div>
          </div>
          <p className="text-center text-[9px] text-slate-600 mt-4">
            Every stage runs in the Web Audio rendering thread — near-zero added latency beyond hardware buffer
          </p>
        </div>
      </section>

      {/* ── SERMON MASTER + FILE TOOLS ─────────────────────────────────── */}
      <section className="max-w-5xl mx-auto px-6 py-10">
        <div className="grid md:grid-cols-2 gap-6 items-center">
          <div>
            <p className="text-[8px] font-bold tracking-[0.2em] text-amber-400 uppercase mb-2">File Mastering</p>
            <h2 className="text-lg font-black text-white mb-3">Sermon Master Pipeline</h2>
            <p className="text-[11px] text-slate-400 leading-relaxed mb-4">
              One-click mastering for recorded sermons. Three stages run offline via
              OfflineAudioContext — faster than real-time, no dropouts.
            </p>
            <div className="space-y-2">
              {[
                { n: '01', t: 'Auto-denoise', d: 'Learns noise profile from first 500ms, spectral subtraction' },
                { n: '02', t: 'EQ + Dynamics', d: 'HPF → EQ chain → de-esser → compressor → brick-wall limiter' },
                { n: '03', t: 'Normalize', d: 'Peak normalize to −3 dBFS with soft-clipping ceiling' },
              ].map(({ n, t, d }) => (
                <div key={n} className="flex gap-3 items-start">
                  <span className="text-[9px] font-black text-amber-500/50 font-mono shrink-0 mt-0.5">{n}</span>
                  <div>
                    <div className="text-[10px] font-bold text-slate-300">{t}</div>
                    <div className="text-[9px] text-slate-600">{d}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
          <div className="space-y-2">
            <p className="text-[8px] font-bold tracking-[0.2em] text-slate-500 uppercase mb-3">Presets</p>
            {[
              { name: 'Sermon (Warm)', vals: 'HPF 80Hz · Bass +3dB · Presence +2dB · Air +1.5dB · Comp 2.5:1' },
              { name: 'Podcast (Crisp)', vals: 'HPF 80Hz · Bass +2dB · Presence +3dB · Air +2dB · Comp 3:1' },
              { name: 'Broadcast (Punchy)', vals: 'HPF 100Hz · Bass +1.5dB · Presence +2.5dB · Air +2.5dB · Comp 4:1' },
            ].map(p => (
              <div key={p.name} className="rounded-lg border border-slate-800 p-3" style={{ background: '#080E1F' }}>
                <div className="text-[10px] font-bold text-white mb-0.5">{p.name}</div>
                <div className="text-[8px] font-mono text-slate-600">{p.vals}</div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ── MORE FEATURES GRID ──────────────────────────────────────────── */}
      <section className="border-t border-slate-800/60 py-10" style={{ background: 'rgba(8,14,31,0.3)' }}>
        <div className="max-w-5xl mx-auto px-6">
          <div className="text-center mb-6">
            <p className="text-[8px] font-bold tracking-[0.2em] text-amber-400 uppercase mb-1">Full Feature Set</p>
            <h2 className="text-lg font-black text-white">Everything You Need on Sunday</h2>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-3 gap-3">
            {[
              { icon: <Globe size={13}/>, l: 'Multi-theme UI', d: '6 color themes — amber, cyan, emerald, violet, rose, slate' },
              { icon: <Shield size={13}/>, l: 'Stream Guard', d: 'YouTube / Facebook / Zoom LUFS targets with automatic ceiling control' },
              { icon: <AudioLines size={13}/>, l: 'Waveform Editor', d: 'Audacity-style cut, copy, paste, fade, silence trim, multi-track' },
              { icon: <Mic size={13}/>, l: 'Auto Calibrate', d: 'Learn ambient noise floor and gate threshold in 3 seconds' },
              { icon: <Activity size={13}/>, l: 'Analytics', d: 'Local session tracking — engine starts, recordings, exports' },
              { icon: <Layers size={13}/>, l: 'VB-CABLE Routing', d: 'Auto-detects virtual cable, shows TIWATON signal chain in OBS' },
              { icon: <Volume2 size={13}/>, l: 'Music Ducking', d: 'Gain-rides worship music down when voice activity detected' },
              { icon: <Sliders size={13}/>, l: 'Smart Auto-Mix', d: 'RMS-driven gain rider keeps pastor consistently loud and present' },
              { icon: <Zap size={13}/>, l: 'Mic Fingerprint', d: '8-band spectral signature learning — reduces false gate triggers' },
            ].map(f => (
              <div key={f.l} className="flex gap-3 p-3 rounded-xl border border-slate-800 hover:border-slate-700 transition-colors"
                style={{ background: '#080E1F' }}>
                <div className="shrink-0 mt-0.5 text-amber-400">{f.icon}</div>
                <div>
                  <div className="text-[10px] font-bold text-slate-200">{f.l}</div>
                  <div className="text-[9px] text-slate-600 mt-0.5 leading-relaxed">{f.d}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ── DOWNLOAD ────────────────────────────────────────────────────── */}
      <section className="max-w-5xl mx-auto px-6 py-14">
        <div className="rounded-2xl border border-amber-500/20 p-8 text-center relative overflow-hidden"
          style={{ background: 'linear-gradient(135deg,#080E1F 0%,#0D1428 100%)' }}>
          <div className="absolute inset-0 pointer-events-none">
            <div className="absolute bottom-0 left-1/2 -translate-x-1/2 w-[400px] h-[200px] rounded-full blur-[100px] opacity-8"
              style={{ background: 'radial-gradient(ellipse,#F59E0B 0%,transparent 70%)' }} />
          </div>
          <div className="relative">
            <p className="text-[8px] font-bold tracking-[0.2em] text-amber-400 uppercase mb-2">Free & Open Source</p>
            <h2 className="text-xl font-black text-white mb-2">Download TIWATON AI Studio</h2>
            <p className="text-[11px] text-slate-400 mb-6 max-w-md mx-auto">
              Available as a desktop app for Windows, macOS, and Linux. Or run it directly in your browser.
            </p>
            <div className="flex items-center justify-center gap-3 flex-wrap mb-6">
              <a href={RELEASES}
                className="flex items-center gap-2 px-5 py-2.5 rounded-xl border border-slate-700 text-[11px] font-bold text-white hover:border-amber-500/40 transition-all hover:scale-105"
                style={{ background: 'rgba(255,255,255,0.05)' }}
                target="_blank" rel="noreferrer">
                <Download size={12} />
                Windows <span className="text-slate-500 font-normal">.exe</span>
              </a>
              <a href={RELEASES}
                className="flex items-center gap-2 px-5 py-2.5 rounded-xl border border-slate-700 text-[11px] font-bold text-white hover:border-amber-500/40 transition-all hover:scale-105"
                style={{ background: 'rgba(255,255,255,0.05)' }}
                target="_blank" rel="noreferrer">
                <Download size={12} />
                macOS <span className="text-slate-500 font-normal">.dmg</span>
              </a>
              <a href={RELEASES}
                className="flex items-center gap-2 px-5 py-2.5 rounded-xl border border-slate-700 text-[11px] font-bold text-white hover:border-amber-500/40 transition-all hover:scale-105"
                style={{ background: 'rgba(255,255,255,0.05)' }}
                target="_blank" rel="noreferrer">
                <Download size={12} />
                Linux <span className="text-slate-500 font-normal">.AppImage</span>
              </a>
            </div>
            <div className="flex items-center justify-center gap-4 text-[9px] text-slate-600">
              <span className="flex items-center gap-1"><Check size={10} className="text-green-500"/>Free forever</span>
              <span className="flex items-center gap-1"><Check size={10} className="text-green-500"/>No account needed</span>
              <span className="flex items-center gap-1"><Check size={10} className="text-green-500"/>No telemetry</span>
              <span className="flex items-center gap-1"><Check size={10} className="text-green-500"/>MIT License</span>
            </div>
            <div className="mt-4 pt-4 border-t border-slate-800">
              <button onClick={onLaunch}
                className="text-[10px] text-amber-400 hover:text-amber-300 transition-colors flex items-center gap-1 mx-auto">
                Or use the web version — no install needed <ChevronRight size={10} />
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* ── FOOTER ──────────────────────────────────────────────────────── */}
      <footer className="border-t border-slate-800/60 py-6" style={{ background: 'rgba(2,6,23,0.8)' }}>
        <div className="max-w-5xl mx-auto px-6 flex items-center justify-between flex-wrap gap-3">
          <div className="flex items-center gap-2">
            <div className="w-4 h-4 rounded flex items-center justify-center" style={{ background: 'rgba(245,158,11,0.15)' }}>
              <Mic size={8} className="text-amber-400" />
            </div>
            <span className="text-[10px] font-black tracking-[0.15em] text-slate-400">TIWATON AI Studio</span>
            <span className="text-[9px] font-mono text-slate-700">v1.4.2</span>
          </div>
          <div className="flex items-center gap-4 text-[9px] text-slate-600">
            <a href={GITHUB} target="_blank" rel="noreferrer" className="hover:text-white transition-colors flex items-center gap-1">
              <Github size={10} /> GitHub
            </a>
            <a href={RELEASES} target="_blank" rel="noreferrer" className="hover:text-white transition-colors">Releases</a>
            <span>MIT License</span>
          </div>
          <p className="text-[9px] text-slate-700 w-full md:w-auto text-center md:text-right">
            Built for churches. Powered by Web Audio API, WebAssembly & AudioWorklets.
          </p>
        </div>
      </footer>
    </div>
  );
}
