import React from 'react';
import {
  ArrowRight,
  CheckCircle2,
  Cpu,
  Headphones,
  Mic,
  Radio,
  Sliders,
  Speaker,
  Volume2,
  Waves,
  Zap,
} from 'lucide-react';

export default function LandingPage({ onEnter }) {
  return (
    <div className="min-h-[100dvh] bg-slate-950 text-white relative overflow-hidden font-sans selection:bg-amber-500 selection:text-white">
      <div className="absolute inset-0 z-0 opacity-20 pointer-events-none">
        <div className="absolute top-[-20%] left-[-10%] w-[600px] h-[600px] bg-amber-600 rounded-full blur-[120px] animate-pulse" />
        <div
          className="absolute bottom-[-25%] right-[-15%] w-[520px] h-[520px] bg-cyan-600 rounded-full blur-[140px]"
          style={{ animationDuration: '4s' }}
        />
      </div>

      <header className="relative z-10">
        <div className="mx-auto max-w-6xl px-6 pt-10 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 bg-gradient-to-tr from-amber-500 to-yellow-600 rounded-xl flex items-center justify-center shadow-[0_0_15px_rgba(var(--accent-rgb),0.6)]">
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
              Hyper-Gate keeps speech present, echo and reverb cleaning controls
              room wash, and smart auto-mixing delivers a broadcast-ready level every
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
                Built for live and file processing
              </span>
              <span className="flex items-center gap-2">
                <CheckCircle2 size={16} className="text-emerald-400" />
                OBS and vMix ready
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
                  title: 'Echo and Reverb Cleaner',
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
                'Clear steps and visible feedback so volunteers feel confident.',
            },
            {
              title: 'AI that serves people',
              description:
                'Enhance the sermon, not the noise. Speech intelligibility stays first.',
            },
            {
              title: 'Massive impact, minimal setup',
              description:
                'Connect a mic, select a mode, and go live. The mix locks in quickly.',
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
                  'Enable Hyper-Gate and AI processors.',
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
                Start a live session or upload your sermon. TIWATON guides the
                settings and delivers a polished mix.
              </p>
            </div>
            <button
              onClick={onEnter}
              className="px-8 py-4 rounded-full bg-amber-500 hover:bg-amber-400 text-white font-semibold shadow-[0_0_15px_rgba(var(--accent-rgb),0.4)]"
            >
              Launch Studio
            </button>
          </div>
        </section>
      </main>
    </div>
  );
}
