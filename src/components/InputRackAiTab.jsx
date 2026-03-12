import { Mic, Radio, Sliders, Speaker, Volume2, Waves } from 'lucide-react';

const nextGenDspFeatures = [
  {
    key: 'lookaheadGate',
    icon: <Waves size={11} />,
    label: 'Lookahead Gate',
    desc: '20ms pre-opens gate - no clipped first syllable',
  },
  {
    key: 'dynamicDeEsser',
    icon: <Mic size={11} />,
    label: 'Dynamic De-esser',
    desc: 'Sidechain sibilance detector - more transparent than notch',
  },
  {
    key: 'multibandComp',
    icon: <Sliders size={11} />,
    label: 'Multiband Compressor',
    desc: '4-band (sub/low/mid/air) independent compression',
  },
  {
    key: 'dereverb',
    icon: <Radio size={11} />,
    label: 'Spectral Dereverberation',
    desc: 'Overlap-add FFT reverb tail subtraction',
  },
];

const aiProcessingFeatures = [
  {
    key: 'voicePattern',
    icon: <Waves size={11} />,
    label: 'Voice Pattern Cleanse',
    desc: 'Removes non-voice spectral content',
  },
  {
    key: 'musicDucking',
    icon: <Volume2 size={11} />,
    label: 'Music Ducking',
    desc: 'Drops music when speech detected',
  },
  {
    key: 'pastorIsolation',
    icon: <Mic size={11} />,
    label: 'Pastor Voice Isolation',
    desc: 'Prioritises pastor over choir',
  },
  {
    key: 'sermonWarmth',
    icon: <Speaker size={11} />,
    label: 'Sermon Warmth',
    desc: 'Low-mid body - podcast feel',
  },
  {
    key: 'smartMixing',
    icon: <Sliders size={11} />,
    label: 'Smart Auto-Mixing',
    desc: 'Continuous level riding',
  },
  {
    key: 'mastering',
    icon: <Volume2 size={11} />,
    label: 'Voice Polish',
    desc: 'Broadcast loudness + sheen',
  },
];

function ToggleCard({ featureKey, icon, label, desc, features, onToggle }) {
  const active = Boolean(features[featureKey]);

  return (
    <div
      onClick={() => onToggle(featureKey)}
      className={`cursor-pointer p-2.5 rounded-lg border transition-all ${
        active
          ? 'border-[var(--accent)]/40'
          : 'border-slate-800 hover:border-slate-700 opacity-60 hover:opacity-100'
      }`}
      style={active ? { background: 'rgba(var(--accent-rgb),0.08)' } : {}}
    >
      <div className="flex items-center justify-between">
        <div
          className={`flex items-center gap-1.5 ${
            active ? 'text-[var(--accent)]' : 'text-slate-500'
          }`}
        >
          {icon}
          <span className="text-[10px] font-semibold text-slate-300">{label}</span>
        </div>
        <div
          className={`w-6 h-3 rounded-full transition-colors relative ${
            active ? 'bg-[var(--accent)]' : 'bg-slate-700'
          }`}
        >
          <div
            className={`absolute top-0.5 w-2 h-2 bg-white rounded-full transition-transform ${
              active ? 'translate-x-3 left-0.5' : 'left-0.5'
            }`}
          />
        </div>
      </div>
      <p className="text-[9px] text-slate-600 mt-1 pl-4">{desc}</p>
    </div>
  );
}

export default function InputRackAiTab({
  features,
  setFeatures,
  setGateMode,
  setSpeakerPreset,
}) {
  const toggleFeature = (featureKey) => {
    setFeatures((prev) => ({
      ...prev,
      [featureKey]: !prev[featureKey],
    }));
  };

  const enableChurchBeta = () => {
    setFeatures((prev) => ({
      ...prev,
      denoise: true,
      dereverb: true,
      voicePattern: true,
      musicDucking: true,
      pastorIsolation: true,
      sermonWarmth: true,
      smartMixing: true,
      mastering: true,
      streamingSafe: true,
      lookaheadGate: true,
      dynamicDeEsser: true,
      multibandComp: true,
    }));
    setGateMode('speech');
    setSpeakerPreset('pastor');
  };

  return (
    <div className="space-y-2">
      <button
        onClick={enableChurchBeta}
        className="w-full py-2 rounded-lg border border-[#00E676]/30 text-[#00E676] text-[9px] font-bold tracking-[0.15em] hover:opacity-90 transition-opacity"
        style={{ background: 'rgba(0,230,118,0.08)' }}
      >
        CHURCH BETA - ALL ON
      </button>
      <p className="text-[9px] text-slate-600 text-center pb-1">
        All AI features + Speech-Only gate + Pastor preset
      </p>

      <div className="flex items-center gap-2 pt-1">
        <div className="flex-1 h-px bg-slate-800" />
        <span className="text-[8px] font-bold tracking-[0.15em] text-slate-600 uppercase">
          Next-Gen DSP
        </span>
        <div className="flex-1 h-px bg-slate-800" />
      </div>

      {nextGenDspFeatures.map((feature) => (
        <ToggleCard
          key={feature.key}
          featureKey={feature.key}
          icon={feature.icon}
          label={feature.label}
          desc={feature.desc}
          features={features}
          onToggle={toggleFeature}
        />
      ))}

      <div className="flex items-center gap-2 pt-1">
        <div className="flex-1 h-px bg-slate-800" />
        <span className="text-[8px] font-bold tracking-[0.15em] text-slate-600 uppercase">
          AI Processing
        </span>
        <div className="flex-1 h-px bg-slate-800" />
      </div>

      {aiProcessingFeatures.map((feature) => (
        <ToggleCard
          key={feature.key}
          featureKey={feature.key}
          icon={feature.icon}
          label={feature.label}
          desc={feature.desc}
          features={features}
          onToggle={toggleFeature}
        />
      ))}
    </div>
  );
}
