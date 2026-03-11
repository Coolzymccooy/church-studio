import React, { useState, useRef, useCallback, useEffect } from 'react';
import Anthropic from '@anthropic-ai/sdk';
import { Mic, Sparkles, Send, X, ChevronDown, ChevronUp, Cpu, BookOpen, Sliders } from 'lucide-react';

// ─── TIWATON AI Assistant ─────────────────────────────────────────────────────
// Powered by Claude. Provides:
//  1. Preset suggestions based on described audio environment
//  2. Audio analysis report from spectrum stats
//  3. Sermon transcription assistance (text Q&A about content)

const SYSTEM_PROMPT = `You are TIWATON AI, the built-in assistant for TIWATON AI Studio — a professional church audio processing workstation. You specialize in:
- Church audio engineering (microphones, room acoustics, live mixing)
- Podcast-quality voice processing (EQ, compression, de-essing, noise gates)
- Recommending settings for the TIWATON DSP pipeline:
  Features: Hyper-Gate (noise gate), Pastor Voice Isolation, Sermon Warmth (EQ), Voice Polish (mastering), Echo/Reverb Cleaner, Music Ducking, Smart Auto-Mixing
  Speaker presets: balanced, pastor, guest, choir
- Sermon transcription, summarization, and content assistance
- OBS/streaming setup and virtual audio cable routing

Keep responses concise. When suggesting preset changes, format them clearly. Be warm, professional, and knowledgeable about church tech.`;

const PRESET_EXAMPLES = {
  preset: {
    label: 'Suggest Preset',
    prompt: 'Based on my setup, which speaker preset and AI features should I enable? My pastor speaks with a deep voice in a medium-sized hall with some echo.',
  },
  noisy: {
    label: 'Too Much Noise',
    prompt: 'I\'m getting background noise and room reverb in my recording. What TIWATON settings should I adjust?',
  },
  podcast: {
    label: 'Podcast Quality',
    prompt: 'How do I make my church recording sound podcast-quality with TIWATON?',
  },
  streaming: {
    label: 'Streaming Setup',
    prompt: 'How do I route TIWATON audio output to OBS for live streaming?',
  },
};

export default function AIAssistant({ isLive, features, setFeatures, currentPreset, spectrumData, isOpen, onClose }) {
  const [messages, setMessages] = useState([
    {
      role: 'assistant',
      content: 'Hello! I\'m your TIWATON AI assistant. I can help you optimize your audio settings, suggest presets for your environment, or answer questions about church audio. What would you like help with?',
    },
  ]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [apiKey, setApiKey] = useState(() => localStorage.getItem('tiwaton_claude_key') || '');
  const [showKeyInput, setShowKeyInput] = useState(false);
  const [activeTab, setActiveTab] = useState('chat'); // 'chat' | 'analyze' | 'presets'
  const messagesEndRef = useRef(null);
  const clientRef = useRef(null);

  useEffect(() => {
    if (apiKey) {
      clientRef.current = new Anthropic({ apiKey, dangerouslyAllowBrowser: true });
      localStorage.setItem('tiwaton_claude_key', apiKey);
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

      // Build context about current state
      const contextBlock = `
[Current TIWATON State]
- Engine: ${isLive ? 'LIVE' : 'Stopped'}
- Speaker Preset: ${currentPreset || 'balanced'}
- Active Features: ${Object.entries(features || {}).filter(([,v]) => v).map(([k]) => k).join(', ') || 'none'}
${spectrumData ? `- Spectrum peak: ${spectrumData.peak?.toFixed(1)}dBFS, Noise floor: ${spectrumData.noiseFloor?.toFixed(1)}dBFS` : ''}
`;

      const apiMessages = [
        { role: 'user', content: contextBlock + '\n\nUser question: ' + updatedMessages[updatedMessages.length - 1].content },
        ...updatedMessages.slice(1, -1).map(m => ({ role: m.role, content: m.content })),
      ].slice(0, 20); // keep context reasonable

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

      // Auto-apply preset suggestions if detected
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
    // If Claude recommends enabling specific features, apply them
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
    if (!spectrumData) {
      sendMessage('Analyze my audio setup and suggest optimal settings. I don\'t have spectrum data available right now.');
      return;
    }
    const analysisPrompt = `Please analyze these audio metrics from my church setup and give me specific recommendations:
- Current preset: ${currentPreset}
- Peak level: ${spectrumData.peak?.toFixed(1) || 'unknown'}dBFS
- Estimated noise floor: ${spectrumData.noiseFloor?.toFixed(1) || 'unknown'}dBFS
- Voice activity: ${spectrumData.voicePercent?.toFixed(0) || 'unknown'}%
- Active features: ${Object.entries(features || {}).filter(([,v]) => v).map(([k]) => k).join(', ') || 'none'}

What should I adjust for the best church broadcast quality?`;
    setActiveTab('chat');
    sendMessage(analysisPrompt);
  }, [spectrumData, currentPreset, features, sendMessage]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed right-4 bottom-4 z-50 flex flex-col rounded-xl border border-slate-700 shadow-2xl overflow-hidden"
      style={{ width: 360, height: 520, background: '#0D1428' }}
    >
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2.5 border-b border-slate-700/60 shrink-0" style={{ background: '#030710' }}>
        <div className="w-6 h-6 rounded flex items-center justify-center" style={{ background: 'rgba(245,158,11,0.2)' }}>
          <Sparkles size={12} className="text-[#F59E0B]" />
        </div>
        <div className="flex-1">
          <p className="text-[11px] font-bold text-slate-100 tracking-wide">TIWATON AI</p>
          <p className="text-[9px] text-slate-500">Powered by Claude</p>
        </div>
        {/* Tabs */}
        <div className="flex items-center gap-0.5 mr-1">
          {[
            { id: 'chat', icon: <Send size={9} />, label: 'Chat' },
            { id: 'analyze', icon: <Cpu size={9} />, label: 'Analyze' },
            { id: 'presets', icon: <Sliders size={9} />, label: 'Quick' },
          ].map(tab => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`flex items-center gap-1 px-2 py-0.5 rounded text-[9px] font-semibold transition-colors ${
                activeTab === tab.id ? 'bg-[#F59E0B]/20 text-[#F59E0B]' : 'text-slate-500 hover:text-slate-300'
              }`}
            >
              {tab.icon}
              {tab.label}
            </button>
          ))}
        </div>
        <button onClick={onClose} className="text-slate-600 hover:text-slate-300 transition-colors">
          <X size={14} />
        </button>
      </div>

      {/* API Key Banner */}
      {!apiKey && (
        <div className="px-3 py-2 border-b border-amber-500/20 shrink-0" style={{ background: 'rgba(245,158,11,0.07)' }}>
          <p className="text-[9px] text-amber-400 mb-1.5 font-medium">Anthropic API key required to use AI assistant</p>
          <div className="flex gap-1.5">
            <input
              type="password"
              placeholder="sk-ant-..."
              value={apiKey}
              onChange={e => setApiKey(e.target.value)}
              className="flex-1 px-2 py-1 rounded text-[9px] bg-black/40 border border-slate-700 text-slate-300 placeholder-slate-600 outline-none focus:border-[#F59E0B]/50"
            />
            <button
              onClick={() => setShowKeyInput(false)}
              className="px-2 py-1 rounded text-[9px] font-bold text-white"
              style={{ background: '#F59E0B' }}
            >
              Save
            </button>
          </div>
          <p className="text-[8px] text-slate-600 mt-1">Key saved locally in your browser. Get one at console.anthropic.com</p>
        </div>
      )}

      {/* Quick Prompts Tab */}
      {activeTab === 'presets' && (
        <div className="flex-1 overflow-y-auto p-3 space-y-2">
          <p className="text-[9px] text-slate-500 uppercase tracking-wider mb-3">Quick questions — tap to ask</p>
          {Object.entries(PRESET_EXAMPLES).map(([key, { label, prompt }]) => (
            <button
              key={key}
              onClick={() => { setActiveTab('chat'); sendMessage(prompt); }}
              className="w-full text-left px-3 py-2.5 rounded-lg border border-slate-700 hover:border-[#F59E0B]/40 hover:bg-[#F59E0B]/5 transition-colors"
            >
              <p className="text-[10px] font-semibold text-slate-200 mb-0.5">{label}</p>
              <p className="text-[9px] text-slate-500 leading-relaxed">{prompt.slice(0, 80)}…</p>
            </button>
          ))}
          <div className="mt-4 pt-3 border-t border-slate-800">
            <button
              onClick={runAudioAnalysis}
              className="w-full flex items-center justify-center gap-2 py-2 rounded-lg border border-[#F59E0B]/30 text-[10px] font-bold text-[#F59E0B] hover:bg-[#F59E0B]/10 transition-colors"
            >
              <Cpu size={10} />
              Analyze Current Audio State
            </button>
          </div>
        </div>
      )}

      {/* Analyze Tab */}
      {activeTab === 'analyze' && (
        <div className="flex-1 overflow-y-auto p-3 space-y-3">
          <p className="text-[9px] text-slate-500 uppercase tracking-wider">Current Engine State</p>

          <div className="space-y-1.5">
            {[
              { label: 'Engine', value: isLive ? 'LIVE' : 'Stopped', color: isLive ? '#00E676' : '#FF5252' },
              { label: 'Preset', value: currentPreset || 'balanced', color: '#F59E0B' },
              { label: 'Active Features', value: Object.entries(features || {}).filter(([,v]) => v).length + ' enabled', color: '#94a3b8' },
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
            <p className="text-[9px] text-slate-500 uppercase tracking-wider">Active Features</p>
            {Object.entries(features || {}).map(([key, val]) => (
              <div key={key} className="flex justify-between items-center py-1 border-b border-slate-800/40">
                <span className="text-[9px] text-slate-400">{key}</span>
                <span className={`text-[8px] font-bold px-1.5 py-0.5 rounded ${val ? 'text-[#00E676] bg-[#00E676]/10' : 'text-slate-600 bg-slate-800/60'}`}>
                  {val ? 'ON' : 'OFF'}
                </span>
              </div>
            ))}
          </div>

          <button
            onClick={runAudioAnalysis}
            className="w-full flex items-center justify-center gap-2 py-2.5 rounded-lg border border-[#F59E0B]/30 text-[10px] font-bold text-[#F59E0B] hover:bg-[#F59E0B]/10 transition-colors"
          >
            <Sparkles size={11} />
            Ask Claude to Analyze &amp; Optimize
          </button>
        </div>
      )}

      {/* Chat Tab */}
      {activeTab === 'chat' && (
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
                    {[0,1,2].map(i => (
                      <div
                        key={i}
                        className="w-1.5 h-1.5 rounded-full bg-[#F59E0B] animate-bounce"
                        style={{ animationDelay: `${i * 0.15}s` }}
                      />
                    ))}
                  </div>
                </div>
              </div>
            )}
            <div ref={messagesEndRef} />
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
                placeholder="Ask about audio settings, presets, or church tech…"
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
            {apiKey && (
              <button
                onClick={() => { setApiKey(''); localStorage.removeItem('tiwaton_claude_key'); }}
                className="mt-1.5 text-[8px] text-slate-700 hover:text-slate-500 transition-colors"
              >
                Clear API key
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}
