import React, { createContext, useContext, useState, useEffect } from 'react';

export const THEMES = {
  amber: {
    id: 'amber',
    name: 'Amber Pro',
    desc: 'Professional broadcast (default)',
    accent: '#F59E0B',
    accentRgb: '245,158,11',
    accentDark: '#D97706',
    accentDarker: '#92400E',
    preview: ['#F59E0B', '#D97706', '#92400E'],
  },
  cyan: {
    id: 'cyan',
    name: 'Cyan Studio',
    desc: 'Cool studio blue',
    accent: '#06B6D4',
    accentRgb: '6,182,212',
    accentDark: '#0891B2',
    accentDarker: '#0E7490',
    preview: ['#06B6D4', '#0891B2', '#0E7490'],
  },
  emerald: {
    id: 'emerald',
    name: 'Emerald Live',
    desc: 'Broadcast green',
    accent: '#10B981',
    accentRgb: '16,185,129',
    accentDark: '#059669',
    accentDarker: '#065F46',
    preview: ['#10B981', '#059669', '#065F46'],
  },
  violet: {
    id: 'violet',
    name: 'Violet Night',
    desc: 'Studio purple',
    accent: '#8B5CF6',
    accentRgb: '139,92,246',
    accentDark: '#7C3AED',
    accentDarker: '#4C1D95',
    preview: ['#8B5CF6', '#7C3AED', '#4C1D95'],
  },
  rose: {
    id: 'rose',
    name: 'Rose Hot',
    desc: 'Live red energy',
    accent: '#F43F5E',
    accentRgb: '244,63,94',
    accentDark: '#E11D48',
    accentDarker: '#881337',
    preview: ['#F43F5E', '#E11D48', '#881337'],
  },
  slate: {
    id: 'slate',
    name: 'Slate Mono',
    desc: 'Minimal monochrome',
    accent: '#94A3B8',
    accentRgb: '148,163,184',
    accentDark: '#64748B',
    accentDarker: '#334155',
    preview: ['#94A3B8', '#64748B', '#334155'],
  },
};

const ThemeContext = createContext(null);

export function ThemeProvider({ children }) {
  const [themeId, setThemeId] = useState(() => localStorage.getItem('tiwaton_theme') || 'amber');

  const theme = THEMES[themeId] || THEMES.amber;

  useEffect(() => {
    localStorage.setItem('tiwaton_theme', themeId);
    const root = document.documentElement;
    root.style.setProperty('--accent', theme.accent);
    root.style.setProperty('--accent-rgb', theme.accentRgb);
    root.style.setProperty('--accent-dark', theme.accentDark);
    root.style.setProperty('--accent-darker', theme.accentDarker);
    root.setAttribute('data-theme', themeId);
  }, [themeId, theme]);

  return (
    <ThemeContext.Provider value={{ themeId, setThemeId, theme, themes: THEMES }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  return useContext(ThemeContext);
}
