import React, { useState } from 'react';
import { useTheme, THEMES } from '../contexts/ThemeContext';
import { Palette, Check } from 'lucide-react';

export default function ThemeSelector() {
  const { themeId, setThemeId, theme } = useTheme();
  const [open, setOpen] = useState(false);

  return (
    <div className="relative">
      <button
        onClick={() => setOpen(o => !o)}
        title="Change app theme"
        className="flex items-center gap-1.5 px-2 py-1 rounded-lg border border-slate-700 hover:border-slate-600 transition-colors text-slate-400 hover:text-white"
        style={{ background: '#0D1428' }}
      >
        <Palette size={13} />
        <div className="w-3 h-3 rounded-full border border-slate-600" style={{ background: theme.accent }} />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div
            className="absolute right-0 top-8 z-50 rounded-xl border border-slate-700 shadow-2xl overflow-hidden"
            style={{ background: '#0D1428', width: 220 }}
          >
            <div className="px-3 py-2 border-b border-slate-800">
              <p className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">App Theme</p>
            </div>
            <div className="p-2 space-y-1">
              {Object.values(THEMES).map(t => (
                <button
                  key={t.id}
                  onClick={() => { setThemeId(t.id); setOpen(false); }}
                  className="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-lg transition-colors hover:bg-slate-800/60 text-left"
                >
                  <div className="flex gap-0.5 shrink-0">
                    {t.preview.map((c, i) => (
                      <div key={i} className="w-3 h-5 first:rounded-l last:rounded-r" style={{ background: c }} />
                    ))}
                  </div>
                  <div className="flex-1">
                    <p className="text-[11px] font-semibold text-slate-200">{t.name}</p>
                    <p className="text-[9px] text-slate-600">{t.desc}</p>
                  </div>
                  {themeId === t.id && (
                    <Check size={12} style={{ color: t.accent }} />
                  )}
                </button>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
