import React, { useState, useRef, useCallback, useEffect } from 'react';
import Anthropic from '@anthropic-ai/sdk';
import { Sparkles, Send, X, Cpu, Sliders, BookOpen, ChevronDown } from 'lucide-react';

// ─── TIWATON AI Assistant ─────────────────────────────────────────────────────
// Mode A (no API key): Instant knowledge base — pre-written answers by topic
// Mode B (API key set): Live Claude Haiku chat with full engine context

const SYSTEM_PROMPT = `You are TIWATON AI, the built-in assistant for TIWATON AI Studio — a professional church audio processing workstation. You specialize in:
- Church audio engineering (microphones, room acoustics, live mixing)
- Podcast-quality voice processing (EQ, compression, de-essing, noise gates)
- Recommending settings for the TIWATON DSP pipeline:
  Features: Hyper-Gate (noise gate), Pastor Voice Isolation, Sermon Warmth (EQ), Voice Polish (mastering), Echo/Reverb Cleaner, Music Ducking, Smart Auto-Mixing
  Speaker presets: balanced, pastor, guest, choir
- Sermon transcription, summarization, and content assistance
- OBS/streaming setup and virtual audio cable routing

Keep responses concise and practical. When suggesting preset changes, format them clearly. Be warm, professional, and knowledgeable about church tech.`;

// ─── Knowledge base: works with zero API key ──────────────────────────────────
const KNOWLEDGE_BASE = [
  {
    category: 'Getting Started',
    color: '#F59E0B',
    emoji: '🚀',
    items: [
      {
        q: 'Which features should I turn on for Sunday service?',
        a: 'Enable these four in order:\n1) Hyper-Gate — blocks room noise between sentences\n2) Voice Isolation — focuses on the pastor\'s voice only\n3) Sermon Warmth — makes the voice sound fuller and broadcast-ready\n4) Auto-Mix — keeps volume levels consistent automatically\n\nThen run Engine → Auto-Calibrate Gate to teach TIWATON your room\'s noise floor.',
      },
      {
        q: 'Which speaker preset should I choose?',
        a: '"Pastor" — for the lead speaker (single close-mic voice). Best for most services.\n"Balanced" — if you have music + speech mixed together.\n"Guest" — when a visiting speaker with a different voice style is on mic.\n"Choir" — for group vocals or worship team.',
      },
      {
        q: 'What is the correct order to start TIWATON?',
        a: '1. Plug in mic / interface BEFORE opening TIWATON\n2. Open TIWATON → go to Settings → select your interface as Input Source\n3. Set Broadcast Output to VB-CABLE (Windows) or BlackHole (Mac)\n4. Enable features: Hyper-Gate, Isolation, Warmth, Auto-Mix\n5. Engine → Auto-Calibrate Gate\n6. Press the amber LIVE button\n7. Confirm OBS is picking up VB-CABLE',
      },
    ],
  },
  {
    category: 'Audio Problems',
    color: '#FF5252',
    emoji: '🔧',
    items: [
      {
        q: 'Too much background noise or HVAC hum',
        a: 'Enable Hyper-Gate + RNNoise first. Then go to Engine → Auto-Calibrate Gate while the room is at its normal noise level (congregation seated, AC running). This teaches TIWATON where the noise floor is so it gates accurately.',
      },
      {
        q: 'Voice sounds thin, harsh or "radio-ey"',
        a: 'Enable Sermon Warmth — it adds bass body at 150 Hz and presence at 3 kHz. Also enable Voice Polish for final mastering. For recordings, use Audio Editor → Sermon Master → "Sermon (Warm)" preset for the full treatment.',
      },
      {
        q: 'Gate cutting off the pastor mid-sentence',
        a: 'The noise gate threshold is too aggressive. Solutions:\n1. Lower the Noise Threshold slider in Settings\n2. Run Auto-Calibrate Gate again — but this time do it while the congregation is seated (more realistic noise level, not an empty room)\n3. Temporarily disable Hyper-Gate if the problem is severe',
      },
      {
        q: 'Clipping / red meters — audio is too hot',
        a: 'Lower gain at source first: reduce the gain knob on your mixer or audio interface until peaks sit around -12 dBFS. Then lower the input gain slider in TIWATON settings. Never try to fix clipping by turning down software volume after the fact.',
      },
      {
        q: 'Audio sounds "underwater" or muffled after noise removal',
        a: 'The noise reduction setting is too aggressive. In the Audio Editor, reduce the noise reduction from "Heavy" to "Medium" or "Light". For the Sermon Master pipeline, the auto-noise removal targets -12 dB which is a safe level.',
      },
    ],
  },
  {
    category: 'OBS & Streaming',
    color: '#60a5fa',
    emoji: '📡',
    items: [
      {
        q: 'How do I route TIWATON audio into OBS?',
        a: 'Step by step:\n1. TIWATON Settings → Broadcast Output → choose "VB-CABLE Input" (Windows) or "BlackHole 2ch" (Mac)\n2. In OBS → Sources → + → Audio Input Capture → pick VB-CABLE or BlackHole\n3. Delete every other mic source from OBS (webcam mic, laptop mic, direct interface)\n4. Start TIWATON Live → confirm OBS audio meter moves when pastor speaks',
      },
      {
        q: 'Echo / hearing the voice twice on stream',
        a: 'You have two active audio sources in OBS. Open the OBS Audio Mixer, identify every source that is NOT VB-CABLE/BlackHole, and delete them. Common culprits: webcam built-in mic, laptop microphone, interface routed directly to OBS.',
      },
      {
        q: 'No sound on stream at all',
        a: 'Check in this order:\n1. Is TIWATON LIVE on? (status shows green "Live Processing")\n2. Is Broadcast Output set in Settings? (not "Not set")\n3. Is OBS using VB-CABLE/BlackHole as its input — not a physical mic?\n4. Does the OBS audio meter move when the pastor speaks?\nIf yes to all — check your streaming platform\'s audio settings.',
      },
      {
        q: 'Do I need a virtual cable? What is VB-CABLE?',
        a: 'Yes — a virtual cable is required to route TIWATON\'s processed audio into OBS.\n\nVB-CABLE (Windows): Free download at vb-audio.com/Cable — installs in 60 seconds\nBlackHole (Mac): Free at existential.audio/blackhole — also fast\n\nOnce installed, you\'ll see "VB-CABLE Input" or "BlackHole 2ch" appear in TIWATON\'s Broadcast Output dropdown.',
      },
    ],
  },
  {
    category: 'Sermon Master & Recording',
    color: '#00E676',
    emoji: '🎙',
    items: [
      {
        q: 'How do I make a sermon recording sound like a podcast?',
        a: 'Audio Editor tab → Import your WAV or MP3 → Choose a preset:\n• "Sermon (Warm)" — intimate, natural church feel\n• "Podcast (Crisp)" — bright presence, more compression\n• "Broadcast (Punchy)" — aggressive, radio-ready\n\nClick Sermon Master → wait a few seconds → Export WAV.\n\nIt runs: noise removal → EQ → compression → de-essing → brick-wall limiting → normalization to -3 dBFS.',
      },
      {
        q: 'How do I record during a live service?',
        a: 'While TIWATON is Live, use the Record button. The recording captures TIWATON\'s processed output (not raw mic). After the service:\n1. Go to Audio Editor tab\n2. The recording loads automatically, or use Import\n3. Apply Sermon Master\n4. Export WAV — broadcast-ready',
      },
      {
        q: 'What does each Sermon Master preset do differently?',
        a: 'Sermon (Warm): +3dB bass at 150Hz, gentle 2.5:1 compression, -3dB de-esser. Best for intimate/conversational preaching style.\n\nPodcast (Crisp): Brighter presence boost at 3.5kHz, 3:1 compression. Best for long-form listening on Spotify/Apple Podcasts.\n\nBroadcast (Punchy): Aggressive 4:1 compression, high-shelf air boost. Best for YouTube/radio-style distribution.',
      },
    ],
  },
  {
    category: 'Settings & Devices',
    color: '#a78bfa',
    emoji: '⚙️',
    items: [
      {
        q: 'My interface isn\'t showing up in the device list',
        a: 'Click the refresh icon (↺) next to Device Settings title. If still missing:\n1. Unplug and replug the USB cable\n2. Check Windows Sound settings — interface should appear as a recording device\n3. Make sure you clicked "Allow" when the browser asked for microphone permission\n4. Try a different USB port',
      },
      {
        q: 'What sample rate should I be using?',
        a: '44,100 Hz (44.1 kHz) is standard for everything — streaming, podcasts, YouTube. TIWATON will show a warning if your device is running below this. 48 kHz is also fine. Never use 8 kHz or 16 kHz for voice (it will sound muffled).',
      },
    ],
  },
];

// Expandable Q&A card for knowledge base
function QACard({ q, a }) {
  const [open, setOpen] = useState(false);
  return (
    <button
      onClick={() => setOpen(o => !o)}
      className="w-full text-left rounded-xl border overflow-hidden transition-all"
      style={{ borderColor: open ? 'rgba(245,158,11,0.35)' : 'rgba(255,255,255,0.06)', background: '#0D1428' }}
    >
      <div className="flex items-start gap-2.5 px-3 py-2.5">
        <span className="text-[10px] font-semibold text-slate-300 flex-1 leading-snug">{q}</span>
        <ChevronDown
          size={13}
          className="text-slate-600 shrink-0 mt-0.5 transition-transform duration-200"
          style={{ transform: open ? 'rotate(180deg)' : 'rotate(0deg)' }}
        />
      </div>
      {open && (
        <div className="px-3 pb-3 pt-1 border-t" style={{ borderColor: 'rgba(255,255,255,0.06)' }}>
          <p className="text-[10px] text-slate-400 leading-relaxed whitespace-pre-line">{a}</p>
        </div>
      )}
    </button>
  );
}

export default function AIAssistant({ isLive, features, setFeatures, currentPreset, spectrumData, isOpen, onClose }) {
  const [messages, setMessages] = useState([
    {
      role: 'assistant',
      content: 'Hello! I\'m your TIWATON AI assistant. I can help you optimize your audio settings, suggest presets, or answer questions about church audio. What would you like help with?',
    },
  ]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [apiKey, setApiKey] = useState(() => localStorage.getItem('tiwaton_claude_key') || '');
  const [showKeyInput, setShowKeyInput] = useState(false);
  const [activeTab, setActiveTab] = useState('kb'); // 'kb' | 'chat' | 'analyze'
  const [kbCategory, setKbCategory] = useState(null);
  const messagesEndRef = useRef(null);
  const clientRef = useRef(null);

  useEffect(() => {
    if (apiKey) {
      clientRef.current = new Anthropic({ apiKey, dangerouslyAllowBrowser: true });
      localStorage.setItem('tiwaton_claude_key', apiKey);
      // Switch to chat tab when key is added
      setActiveTab('chat');
    }
  }, [apiKey]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const sendMessage = useCallback(async (text) => {
    if (!text.trim()) return;
    if (!apiKey) {
      setShowKeyInput(true);
      return;
    }

    const userMsg = { role: 'user', content: text };
    const updatedMessages = [...messages, userMsg];
    setMessages(updatedMessages);
    setInput('');
    setIsLoading(true);

    try {
      const client = clientRef.current || new Anthropic({ apiKey, dangerouslyAllowBrowser: true });

      const contextBlock = `[Current TIWATON State]
- Engine: ${isLive ? 'LIVE' : 'Stopped'}
- Speaker Preset: ${currentPreset || 'balanced'}
- Active Features: ${Object.entries(features || {}).filter(([, v]) => v).map(([k]) => k).join(', ') || 'none'}
${spectrumData ? `- Spectrum peak: ${spectrumData.peak?.toFixed(1)}dBFS, Noise floor: ${spectrumData.noiseFloor?.toFixed(1)}dBFS` : ''}`;

      const response = await client.messages.create({
        model: 'claude-haiku-4-5-20251001',
        max_tokens: 800,
        system: SYSTEM_PROMPT,
        messages: [
          { role: 'user', content: contextBlock + '\n\nUser: ' + text },
        ],
      });

      const assistantText = response.content[0]?.text || 'Sorry, I couldn\'t generate a response.';
      setMessages(prev => [...prev, { role: 'assistant', content: assistantText }]);
      applyInlinePresetIfDetected(assistantText);
    } catch (err) {
      const errMsg = err.status === 401
        ? 'Invalid API key. Please check your Anthropic API key.'
        : `Error: ${err.message || 'Failed to connect to Claude'}`;
      setMessages(prev => [...prev, { role: 'assistant', content: errMsg, isError: true }]);
    } finally {
      setIsLoading(false);
    }
  }, [apiKey, messages, isLive, features, currentPreset, spectrumData]);

  const applyInlinePresetIfDetected = (text) => {
    if (!setFeatures) return;
    const lower = text.toLowerCase();
    if (lower.includes('enable hyper-gate') || lower.includes('turn on hyper-gate')) {
      setFeatures(f => ({ ...f, denoise: true }));
    }
    if (lower.includes('enable pastor voice') || lower.includes('pastor isolation')) {
      setFeatures(f => ({ ...f, pastorIsolation: true }));
    }
    if (lower.includes('enable music ducking') || lower.includes('turn on ducking')) {
      setFeatures(f => ({ ...f, musicDucking: true }));
    }
  };

  const runAudioAnalysis = useCallback(async () => {
    const analysisPrompt = spectrumData
      ? `Analyze these audio metrics from my church setup:\n- Preset: ${currentPreset}\n- Peak: ${spectrumData.peak?.toFixed(1) || 'unknown'} dBFS\n- Noise floor: ${spectrumData.noiseFloor?.toFixed(1) || 'unknown'} dBFS\n- Voice activity: ${spectrumData.voicePercent?.toFixed(0) || 'unknown'}%\n- Active features: ${Object.entries(features || {}).filter(([, v]) => v).map(([k]) => k).join(', ') || 'none'}\n\nWhat should I adjust for best church broadcast quality?`
      : `My TIWATON setup: preset=${currentPreset}, features active=${Object.entries(features || {}).filter(([, v]) => v).map(([k]) => k).join(', ') || 'none'}. Suggest optimal settings for a Sunday live stream.`;
    setActiveTab('chat');
    sendMessage(analysisPrompt);
  }, [spectrumData, currentPreset, features, sendMessage]);

  if (!isOpen) return null;

  const hasKey = !!apiKey;
  const tabs = [
    { id: 'kb',      icon: <BookOpen size={9} />,  label: 'Help' },
    { id: 'chat',    icon: <Send size={9} />,       label: 'AI Chat' },
    { id: 'analyze', icon: <Cpu size={9} />,        label: 'Analyze' },
  ];

  return (
    <div
      className="fixed right-4 bottom-4 z-50 flex flex-col rounded-xl border border-slate-700 shadow-2xl overflow-hidden"
      style={{ width: 370, height: 540, background: '#0D1428' }}
    >
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2.5 border-b border-slate-700/60 shrink-0" style={{ background: '#030710' }}>
        <div className="w-6 h-6 rounded flex items-center justify-center" style={{ background: 'rgba(245,158,11,0.2)' }}>
          <Sparkles size={12} className="text-[#F59E0B]" />
        </div>
        <div className="flex-1">
          <p className="text-[11px] font-bold text-slate-100 tracking-wide">TIWATON AI</p>
          <p className="text-[9px] text-slate-500">{hasKey ? 'Powered by Claude' : 'Knowledge Base Mode'}</p>
        </div>
        <div className="flex items-center gap-0.5 mr-1">
          {tabs.map(tab => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`flex items-center gap-1 px-2 py-0.5 rounded text-[9px] font-semibold transition-colors ${
                activeTab === tab.id ? 'bg-[#F59E0B]/20 text-[#F59E0B]' : 'text-slate-500 hover:text-slate-300'
              }`}
            >
              {tab.icon}
              {tab.label}
              {tab.id === 'chat' && !hasKey && (
                <span className="ml-0.5 text-[7px] text-amber-600 font-bold">KEY</span>
              )}
            </button>
          ))}
        </div>
        <button onClick={onClose} className="text-slate-600 hover:text-slate-300 transition-colors">
          <X size={14} />
        </button>
      </div>

      {/* ── KNOWLEDGE BASE TAB (no API key needed) ── */}
      {activeTab === 'kb' && (
        <div className="flex-1 overflow-y-auto">
          {kbCategory === null ? (
            // Category picker
            <div className="p-3 space-y-2">
              <p className="text-[9px] text-slate-600 uppercase tracking-wider mb-3">Browse by topic — no API key needed</p>
              {KNOWLEDGE_BASE.map(cat => (
                <button
                  key={cat.category}
                  onClick={() => setKbCategory(cat.category)}
                  className="w-full flex items-center gap-3 px-3 py-3 rounded-xl border border-slate-800 hover:border-slate-700 text-left transition-all"
                  style={{ background: '#080E1F' }}
                >
                  <span className="text-xl leading-none">{cat.emoji}</span>
                  <div className="flex-1">
                    <p className="text-[11px] font-semibold" style={{ color: cat.color }}>{cat.category}</p>
                    <p className="text-[9px] text-slate-600">{cat.items.length} answers</p>
                  </div>
                  <span className="text-slate-700 text-base">›</span>
                </button>
              ))}

              {/* Unlock AI chat CTA */}
              {!hasKey && (
                <div
                  className="mt-3 rounded-xl border px-3 py-3"
                  style={{ background: 'rgba(245,158,11,0.05)', borderColor: 'rgba(245,158,11,0.2)' }}
                >
                  <p className="text-[10px] font-bold text-amber-400 mb-1">Unlock AI Chat</p>
                  <p className="text-[9px] text-slate-500 mb-2 leading-relaxed">
                    Add your Anthropic API key to get live answers from Claude about your specific audio situation.
                  </p>
                  {showKeyInput ? (
                    <div className="flex gap-1.5">
                      <input
                        type="password"
                        placeholder="sk-ant-..."
                        value={apiKey}
                        onChange={e => setApiKey(e.target.value)}
                        className="flex-1 px-2 py-1.5 rounded-lg text-[9px] bg-black/40 border border-slate-700 text-slate-300 placeholder-slate-600 outline-none focus:border-[#F59E0B]/50"
                      />
                      <button
                        onClick={() => setShowKeyInput(false)}
                        className="px-2.5 py-1.5 rounded-lg text-[9px] font-bold text-black"
                        style={{ background: '#F59E0B' }}
                      >
                        Save
                      </button>
                    </div>
                  ) : (
                    <button
                      onClick={() => setShowKeyInput(true)}
                      className="text-[9px] font-semibold text-amber-500 hover:text-amber-400 underline"
                    >
                      Enter API key →
                    </button>
                  )}
                </div>
              )}
            </div>
          ) : (
            // Q&A list for selected category
            <div className="p-3">
              <button
                onClick={() => setKbCategory(null)}
                className="flex items-center gap-1.5 text-[9px] text-slate-500 hover:text-slate-300 mb-3"
              >
                ‹ All topics
              </button>
              {(() => {
                const cat = KNOWLEDGE_BASE.find(c => c.category === kbCategory);
                return (
                  <>
                    <div className="flex items-center gap-2 mb-3">
                      <span className="text-lg">{cat.emoji}</span>
                      <p className="text-[11px] font-bold" style={{ color: cat.color }}>{cat.category}</p>
                    </div>
                    <div className="space-y-2">
                      {cat.items.map(item => (
                        <QACard key={item.q} q={item.q} a={item.a} />
                      ))}
                    </div>
                  </>
                );
              })()}
            </div>
          )}
        </div>
      )}

      {/* ── AI CHAT TAB ── */}
      {activeTab === 'chat' && (
        <>
          {/* API key gate */}
          {!hasKey && (
            <div className="flex-1 flex flex-col items-center justify-center p-5 text-center gap-3">
              <div className="w-12 h-12 rounded-xl flex items-center justify-center" style={{ background: 'rgba(245,158,11,0.1)', border: '1px solid rgba(245,158,11,0.2)' }}>
                <Sparkles size={20} className="text-amber-400" />
              </div>
              <div>
                <p className="text-[12px] font-bold text-slate-100 mb-1">AI Chat requires an API key</p>
                <p className="text-[10px] text-slate-500 leading-relaxed">
                  Get a free key at <span className="text-amber-400">console.anthropic.com</span> — takes 2 minutes. Or browse the Help tab for instant answers.
                </p>
              </div>
              {showKeyInput ? (
                <div className="w-full flex gap-1.5">
                  <input
                    type="password"
                    placeholder="sk-ant-api03-..."
                    value={apiKey}
                    onChange={e => setApiKey(e.target.value)}
                    className="flex-1 px-2.5 py-2 rounded-lg text-[10px] bg-black/40 border border-slate-700 text-slate-300 placeholder-slate-600 outline-none focus:border-[#F59E0B]/50"
                  />
                  <button
                    onClick={() => setShowKeyInput(false)}
                    className="px-3 py-2 rounded-lg text-[10px] font-bold text-black"
                    style={{ background: '#F59E0B' }}
                  >
                    Save
                  </button>
                </div>
              ) : (
                <button
                  onClick={() => setShowKeyInput(true)}
                  className="px-4 py-2 rounded-lg text-[11px] font-bold text-black"
                  style={{ background: '#F59E0B' }}
                >
                  Enter API Key
                </button>
              )}
              <button onClick={() => setActiveTab('kb')} className="text-[9px] text-slate-600 hover:text-slate-400">
                Browse Help tab instead →
              </button>
            </div>
          )}

          {hasKey && (
            <>
              {/* Messages */}
              <div className="flex-1 overflow-y-auto p-3 space-y-3">
                {messages.map((msg, i) => (
                  <div key={i} className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                    {msg.role === 'assistant' && (
                      <div className="w-5 h-5 rounded-full flex items-center justify-center shrink-0 mr-2 mt-0.5" style={{ background: 'rgba(245,158,11,0.2)' }}>
                        <Sparkles size={9} className="text-[#F59E0B]" />
                      </div>
                    )}
                    <div
                      className={`max-w-[85%] px-3 py-2 rounded-lg text-[10px] leading-relaxed ${
                        msg.role === 'user'
                          ? 'text-white rounded-br-none'
                          : msg.isError
                          ? 'border border-red-500/30 text-red-400 rounded-bl-none'
                          : 'text-slate-300 border border-slate-700/60 rounded-bl-none'
                      }`}
                      style={msg.role === 'user' ? { background: '#F59E0B' } : { background: 'rgba(13,20,40,0.8)' }}
                    >
                      <p className="whitespace-pre-wrap">{msg.content}</p>
                    </div>
                  </div>
                ))}
                {isLoading && (
                  <div className="flex justify-start">
                    <div className="w-5 h-5 rounded-full flex items-center justify-center shrink-0 mr-2" style={{ background: 'rgba(245,158,11,0.2)' }}>
                      <Sparkles size={9} className="text-[#F59E0B]" />
                    </div>
                    <div className="px-3 py-2 rounded-lg border border-slate-700/60" style={{ background: 'rgba(13,20,40,0.8)' }}>
                      <div className="flex gap-1">
                        {[0, 1, 2].map(i => (
                          <div key={i} className="w-1.5 h-1.5 rounded-full bg-[#F59E0B] animate-bounce" style={{ animationDelay: `${i * 0.15}s` }} />
                        ))}
                      </div>
                    </div>
                  </div>
                )}
                <div ref={messagesEndRef} />
              </div>

              {/* Quick prompts */}
              <div className="px-3 pb-1 flex gap-1.5 overflow-x-auto shrink-0">
                {[
                  'Suggest settings for my room',
                  'Why is there echo?',
                  'How to fix gate cutting off voice?',
                  'Route to OBS',
                ].map(prompt => (
                  <button
                    key={prompt}
                    onClick={() => sendMessage(prompt)}
                    className="shrink-0 px-2 py-1 rounded-full border border-slate-700 text-[9px] text-slate-400 hover:border-amber-500/40 hover:text-amber-400 transition-colors whitespace-nowrap"
                    style={{ background: '#080E1F' }}
                  >
                    {prompt}
                  </button>
                ))}
              </div>

              {/* Input */}
              <div className="border-t border-slate-700/60 p-2.5 shrink-0" style={{ background: '#030710' }}>
                <div className="flex gap-2 items-end">
                  <textarea
                    value={input}
                    onChange={e => setInput(e.target.value)}
                    onKeyDown={e => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        sendMessage(input);
                      }
                    }}
                    placeholder="Ask about settings, presets, OBS routing…"
                    rows={2}
                    className="flex-1 px-2.5 py-1.5 rounded-lg text-[10px] bg-black/40 border border-slate-700 text-slate-300 placeholder-slate-600 outline-none focus:border-[#F59E0B]/50 resize-none leading-relaxed"
                  />
                  <button
                    onClick={() => sendMessage(input)}
                    disabled={isLoading || !input.trim()}
                    className="w-8 h-8 rounded-lg flex items-center justify-center transition-colors disabled:opacity-40"
                    style={{ background: '#F59E0B' }}
                  >
                    <Send size={12} className="text-white" />
                  </button>
                </div>
                <button
                  onClick={() => { setApiKey(''); localStorage.removeItem('tiwaton_claude_key'); clientRef.current = null; setActiveTab('kb'); }}
                  className="mt-1 text-[8px] text-slate-700 hover:text-slate-500 transition-colors"
                >
                  Clear API key
                </button>
              </div>
            </>
          )}
        </>
      )}

      {/* ── ANALYZE TAB ── */}
      {activeTab === 'analyze' && (
        <div className="flex-1 overflow-y-auto p-3 space-y-3">
          <p className="text-[9px] text-slate-500 uppercase tracking-wider">Current Engine State</p>

          <div className="space-y-1.5">
            {[
              { label: 'Engine', value: isLive ? 'LIVE' : 'Stopped', color: isLive ? '#00E676' : '#FF5252' },
              { label: 'Preset', value: currentPreset || 'balanced', color: '#F59E0B' },
              { label: 'Active Features', value: Object.entries(features || {}).filter(([, v]) => v).length + ' enabled', color: '#94a3b8' },
              spectrumData && { label: 'Peak Level', value: spectrumData.peak?.toFixed(1) + ' dBFS', color: '#94a3b8' },
              spectrumData && { label: 'Voice Activity', value: spectrumData.voicePercent?.toFixed(0) + '%', color: '#94a3b8' },
            ].filter(Boolean).map(item => (
              <div key={item.label} className="flex justify-between items-center py-1.5 border-b border-slate-800/60">
                <span className="text-[9px] text-slate-500">{item.label}</span>
                <span className="text-[9px] font-mono font-bold" style={{ color: item.color }}>{item.value}</span>
              </div>
            ))}
          </div>

          <div className="space-y-1.5">
            <p className="text-[9px] text-slate-500 uppercase tracking-wider">Features</p>
            {Object.entries(features || {}).map(([key, val]) => (
              <div key={key} className="flex justify-between items-center py-1 border-b border-slate-800/40">
                <span className="text-[9px] text-slate-400">{key}</span>
                <span className={`text-[8px] font-bold px-1.5 py-0.5 rounded ${val ? 'text-[#00E676] bg-[#00E676]/10' : 'text-slate-600 bg-slate-800/60'}`}>
                  {val ? 'ON' : 'OFF'}
                </span>
              </div>
            ))}
          </div>

          {hasKey ? (
            <button
              onClick={runAudioAnalysis}
              className="w-full flex items-center justify-center gap-2 py-2.5 rounded-lg border border-[#F59E0B]/30 text-[10px] font-bold text-[#F59E0B] hover:bg-[#F59E0B]/10 transition-colors"
            >
              <Sparkles size={11} />
              Ask Claude to Analyze & Optimize
            </button>
          ) : (
            <div className="rounded-lg border border-slate-800 px-3 py-2 text-center" style={{ background: '#080E1F' }}>
              <p className="text-[9px] text-slate-600">Add an API key in the AI Chat tab to get personalized Claude analysis</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
