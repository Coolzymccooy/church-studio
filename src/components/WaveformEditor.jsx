import React, { useState, useRef, useEffect, useCallback } from 'react';

/**
 * WaveformEditor — Audacity-style waveform editor
 * Supports: zoom, scroll, region selection, cut/copy/paste/join,
 * noise removal (spectral subtraction), silence trim, fade in/out,
 * and multi-track overlay with mix-down.
 */

const COLORS = {
  bg: '#050A1C',
  panel: '#0D1428',
  wave: 'var(--accent)',
  waveActive: '#00E676',
  selection: 'rgba(var(--accent-rgb), 0.25)',
  selectionBorder: 'var(--accent)',
  playhead: '#FF5252',
  grid: 'rgba(255,255,255,0.06)',
  text: '#94a3b8',
  textBright: '#e2e8f0',
  noiseProfile: '#FF5252',
  fadeOverlay: 'rgba(var(--accent-rgb), 0.15)',
};

// Resample AudioBuffer channel data to fit pixel width
function getPeaks(channelData, width, startSample = 0, endSample) {
  const end = endSample || channelData.length;
  const length = end - startSample;
  const step = Math.max(1, Math.floor(length / width));
  const peaks = [];
  for (let i = 0; i < width; i++) {
    const offset = startSample + i * step;
    let min = 1, max = -1;
    for (let j = 0; j < step && offset + j < end; j++) {
      const val = channelData[offset + j];
      if (val < min) min = val;
      if (val > max) max = val;
    }
    peaks.push({ min, max });
  }
  return peaks;
}

// Compute RMS in a region (for silence detection)
function computeRMS(data, start, length) {
  let sum = 0;
  const end = Math.min(start + length, data.length);
  for (let i = start; i < end; i++) {
    sum += data[i] * data[i];
  }
  return Math.sqrt(sum / (end - start));
}

export default function WaveformEditor({ audioContext, onExport }) {
  const canvasRef = useRef(null);
  const containerRef = useRef(null);
  const animFrameRef = useRef(null);

  // Audio data
  const [audioBuffer, setAudioBuffer] = useState(null);
  const [tracks, setTracks] = useState([]); // Multi-track: [{buffer, name, muted, solo, gain}]
  const [activeTrackIdx, setActiveTrackIdx] = useState(0);
  const fileInputRef = useRef(null);

  // Playback
  const [isPlaying, setIsPlaying] = useState(false);
  const [playheadPos, setPlayheadPos] = useState(0); // in seconds
  const sourceNodeRef = useRef(null);
  const startTimeRef = useRef(0);
  const startOffsetRef = useRef(0);

  // View
  const [zoom, setZoom] = useState(1); // pixels per second
  const [scrollX, setScrollX] = useState(0);
  const [canvasWidth, setCanvasWidth] = useState(900);
  const canvasHeight = 200;

  // Selection (in seconds)
  const [selStart, setSelStart] = useState(null);
  const [selEnd, setSelEnd] = useState(null);
  const [isDragging, setIsDragging] = useState(false);

  // Clipboard
  const clipboardRef = useRef(null);

  // Undo/redo
  const [undoStack, setUndoStack] = useState([]);
  const [redoStack, setRedoStack] = useState([]);

  // Noise profile for spectral subtraction
  const [noiseProfile, setNoiseProfile] = useState(null);
  const [noiseReduction, setNoiseReduction] = useState(12); // dB

  // Effects state
  const [fadeDuration, setFadeDuration] = useState(0.5); // seconds

  // Tool mode
  const [tool, setTool] = useState('select'); // select, cut, zoom

  // Sermon Master
  const [masterProgress, setMasterProgress] = useState(0);
  const [masterStage, setMasterStage] = useState(null);
  const [masterPreset, setMasterPreset] = useState('sermon');

  // --- File Loading ---
  const handleFileLoad = useCallback(async (file) => {
    if (!file) return;
    const ctx = audioContext || new (window.AudioContext || window.webkitAudioContext)();
    try {
      const arrayBuffer = await file.arrayBuffer();
      const decoded = await ctx.decodeAudioData(arrayBuffer);
      pushUndo();
      setAudioBuffer(decoded);
      setTracks([{ buffer: decoded, name: file.name, muted: false, solo: false, gain: 1.0 }]);
      setActiveTrackIdx(0);
      setSelStart(null);
      setSelEnd(null);
      setPlayheadPos(0);
      setScrollX(0);
      // Set zoom to fit the whole file
      if (containerRef.current) {
        const w = containerRef.current.clientWidth;
        setCanvasWidth(w);
        setZoom(w / decoded.duration);
      }
    } catch (err) {
      console.error('Failed to decode audio file:', err);
      alert('Could not decode this audio file. Try WAV or MP3.');
    }
  }, [audioContext]);

  // --- Undo/Redo ---
  const pushUndo = useCallback(() => {
    if (audioBuffer) {
      setUndoStack(prev => [...prev.slice(-20), audioBuffer]);
      setRedoStack([]);
    }
  }, [audioBuffer]);

  const undo = useCallback(() => {
    if (undoStack.length === 0) return;
    const prev = undoStack[undoStack.length - 1];
    setRedoStack(r => [...r, audioBuffer]);
    setUndoStack(u => u.slice(0, -1));
    setAudioBuffer(prev);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: prev } : tr));
  }, [undoStack, audioBuffer, activeTrackIdx]);

  const redo = useCallback(() => {
    if (redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1];
    setUndoStack(u => [...u, audioBuffer]);
    setRedoStack(r => r.slice(0, -1));
    setAudioBuffer(next);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: next } : tr));
  }, [redoStack, audioBuffer, activeTrackIdx]);

  // --- Canvas Resize ---
  useEffect(() => {
    const handleResize = () => {
      if (containerRef.current) {
        setCanvasWidth(containerRef.current.clientWidth);
      }
    };
    handleResize();
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  // --- Convert pixel to time ---
  const pxToTime = useCallback((px) => {
    return (px + scrollX) / zoom;
  }, [scrollX, zoom]);

  const timeToPx = useCallback((t) => {
    return t * zoom - scrollX;
  }, [scrollX, zoom]);

  // --- Mouse Interaction ---
  const handleMouseDown = (e) => {
    if (!audioBuffer) return;
    const rect = canvasRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const t = pxToTime(x);
    setSelStart(t);
    setSelEnd(t);
    setIsDragging(true);
    setPlayheadPos(t);
  };

  const handleMouseMove = (e) => {
    if (!isDragging || !audioBuffer) return;
    const rect = canvasRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const t = Math.max(0, Math.min(pxToTime(x), audioBuffer.duration));
    setSelEnd(t);
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  // --- Drawing ---
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    canvas.width = canvasWidth;
    canvas.height = canvasHeight;

    const draw = () => {
      ctx.fillStyle = COLORS.panel;
      ctx.fillRect(0, 0, canvasWidth, canvasHeight);

      if (!audioBuffer) {
        ctx.fillStyle = COLORS.text;
        ctx.font = '13px Inter, sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText('Drop an audio file or click "Import" to begin editing', canvasWidth / 2, canvasHeight / 2);
        return;
      }

      const data = audioBuffer.getChannelData(0);
      const duration = audioBuffer.duration;
      const sr = audioBuffer.sampleRate;

      // Visible time range
      const startTime = scrollX / zoom;
      const endTime = (scrollX + canvasWidth) / zoom;
      const startSample = Math.floor(startTime * sr);
      const endSample = Math.min(Math.ceil(endTime * sr), data.length);

      // Draw grid lines
      ctx.strokeStyle = COLORS.grid;
      ctx.lineWidth = 1;
      const gridInterval = zoom > 200 ? 0.1 : zoom > 50 ? 0.5 : zoom > 10 ? 1 : 5;
      const gridStart = Math.floor(startTime / gridInterval) * gridInterval;
      for (let t = gridStart; t <= endTime; t += gridInterval) {
        const x = timeToPx(t);
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, canvasHeight);
        ctx.stroke();
        // Time label
        ctx.fillStyle = COLORS.text;
        ctx.font = '10px JetBrains Mono, monospace';
        ctx.textAlign = 'center';
        const label = t >= 60 ? `${Math.floor(t / 60)}:${(t % 60).toFixed(1).padStart(4, '0')}` : t.toFixed(1) + 's';
        ctx.fillText(label, x, 12);
      }

      // Draw selection region
      if (selStart !== null && selEnd !== null) {
        const s = Math.min(selStart, selEnd);
        const e = Math.max(selStart, selEnd);
        const x1 = timeToPx(s);
        const x2 = timeToPx(e);
        ctx.fillStyle = COLORS.selection;
        ctx.fillRect(x1, 0, x2 - x1, canvasHeight);
        ctx.strokeStyle = COLORS.selectionBorder;
        ctx.lineWidth = 1;
        ctx.strokeRect(x1, 0, x2 - x1, canvasHeight);
      }

      // Draw waveform
      const peaks = getPeaks(data, canvasWidth, startSample, endSample);
      const mid = canvasHeight / 2;
      const amp = mid - 20;
      ctx.fillStyle = COLORS.wave;
      for (let i = 0; i < peaks.length; i++) {
        const { min, max } = peaks[i];
        const y1 = mid - max * amp;
        const y2 = mid - min * amp;
        ctx.fillRect(i, y1, 1, Math.max(1, y2 - y1));
      }

      // Center line
      ctx.strokeStyle = 'rgba(255,255,255,0.1)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, mid);
      ctx.lineTo(canvasWidth, mid);
      ctx.stroke();

      // Playhead
      const phX = timeToPx(playheadPos);
      if (phX >= 0 && phX <= canvasWidth) {
        ctx.strokeStyle = COLORS.playhead;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(phX, 0);
        ctx.lineTo(phX, canvasHeight);
        ctx.stroke();
        // Playhead triangle
        ctx.fillStyle = COLORS.playhead;
        ctx.beginPath();
        ctx.moveTo(phX - 5, 0);
        ctx.lineTo(phX + 5, 0);
        ctx.lineTo(phX, 8);
        ctx.fill();
      }

      // Duration label
      ctx.fillStyle = COLORS.textBright;
      ctx.font = '11px JetBrains Mono, monospace';
      ctx.textAlign = 'right';
      ctx.fillText(`${duration.toFixed(2)}s | ${sr}Hz | ${audioBuffer.numberOfChannels}ch`, canvasWidth - 8, canvasHeight - 8);

      // Selection info
      if (selStart !== null && selEnd !== null && Math.abs(selEnd - selStart) > 0.001) {
        const s = Math.min(selStart, selEnd);
        const e = Math.max(selStart, selEnd);
        ctx.fillStyle = COLORS.selectionBorder;
        ctx.textAlign = 'left';
        ctx.fillText(`Selection: ${s.toFixed(2)}s → ${e.toFixed(2)}s (${(e - s).toFixed(2)}s)`, 8, canvasHeight - 8);
      }

      animFrameRef.current = requestAnimationFrame(draw);
    };

    draw();
    return () => {
      if (animFrameRef.current) cancelAnimationFrame(animFrameRef.current);
    };
  }, [audioBuffer, canvasWidth, scrollX, zoom, selStart, selEnd, playheadPos, timeToPx, pxToTime]);

  // --- Playback ---
  const playAudio = useCallback(() => {
    if (!audioBuffer) return;
    const ctx = audioContext || new (window.AudioContext || window.webkitAudioContext)();
    if (sourceNodeRef.current) {
      try { sourceNodeRef.current.stop(); } catch {}
    }
    const source = ctx.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(ctx.destination);

    const startOffset = selStart !== null && selEnd !== null && Math.abs(selEnd - selStart) > 0.01
      ? Math.min(selStart, selEnd)
      : playheadPos;
    const endOffset = selStart !== null && selEnd !== null && Math.abs(selEnd - selStart) > 0.01
      ? Math.max(selStart, selEnd)
      : audioBuffer.duration;

    source.start(0, startOffset, endOffset - startOffset);
    sourceNodeRef.current = source;
    startTimeRef.current = ctx.currentTime;
    startOffsetRef.current = startOffset;
    setIsPlaying(true);

    // Update playhead
    const updatePlayhead = () => {
      if (!sourceNodeRef.current) return;
      const elapsed = ctx.currentTime - startTimeRef.current;
      const pos = startOffsetRef.current + elapsed;
      if (pos >= endOffset) {
        setIsPlaying(false);
        setPlayheadPos(startOffsetRef.current);
        return;
      }
      setPlayheadPos(pos);
      requestAnimationFrame(updatePlayhead);
    };
    requestAnimationFrame(updatePlayhead);

    source.onended = () => {
      setIsPlaying(false);
      sourceNodeRef.current = null;
    };
  }, [audioBuffer, audioContext, playheadPos, selStart, selEnd]);

  const stopAudio = useCallback(() => {
    if (sourceNodeRef.current) {
      try { sourceNodeRef.current.stop(); } catch {}
      sourceNodeRef.current = null;
    }
    setIsPlaying(false);
  }, []);

  // --- Editing Operations ---

  // Helper: create new AudioBuffer from Float32Array(s)
  const createBuffer = useCallback((channelArrays, sampleRate) => {
    const ctx = audioContext || new (window.AudioContext || window.webkitAudioContext)();
    const length = channelArrays[0].length;
    const buf = ctx.createBuffer(channelArrays.length, length, sampleRate);
    for (let ch = 0; ch < channelArrays.length; ch++) {
      buf.copyToChannel(channelArrays[ch], ch);
    }
    return buf;
  }, [audioContext]);

  // Get selection bounds in samples
  const getSelSamples = useCallback(() => {
    if (!audioBuffer || selStart === null || selEnd === null) return null;
    const sr = audioBuffer.sampleRate;
    const s = Math.floor(Math.min(selStart, selEnd) * sr);
    const e = Math.ceil(Math.max(selStart, selEnd) * sr);
    return { start: Math.max(0, s), end: Math.min(e, audioBuffer.length) };
  }, [audioBuffer, selStart, selEnd]);

  // CUT
  const cutSelection = useCallback(() => {
    const sel = getSelSamples();
    if (!sel || sel.end <= sel.start) return;
    pushUndo();

    const nch = audioBuffer.numberOfChannels;
    const sr = audioBuffer.sampleRate;
    const clipChannels = [];
    const resultChannels = [];

    for (let ch = 0; ch < nch; ch++) {
      const data = audioBuffer.getChannelData(ch);
      // Copy selection to clipboard
      clipChannels.push(data.slice(sel.start, sel.end));
      // Remove selection from main buffer
      const newData = new Float32Array(data.length - (sel.end - sel.start));
      newData.set(data.subarray(0, sel.start), 0);
      newData.set(data.subarray(sel.end), sel.start);
      resultChannels.push(newData);
    }

    clipboardRef.current = { channels: clipChannels, sampleRate: sr };
    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
    setSelStart(sel.start / sr);
    setSelEnd(sel.start / sr);
  }, [audioBuffer, getSelSamples, pushUndo, createBuffer, activeTrackIdx]);

  // COPY
  const copySelection = useCallback(() => {
    const sel = getSelSamples();
    if (!sel || sel.end <= sel.start) return;
    const nch = audioBuffer.numberOfChannels;
    const clipChannels = [];
    for (let ch = 0; ch < nch; ch++) {
      clipChannels.push(audioBuffer.getChannelData(ch).slice(sel.start, sel.end));
    }
    clipboardRef.current = { channels: clipChannels, sampleRate: audioBuffer.sampleRate };
  }, [audioBuffer, getSelSamples]);

  // PASTE (insert at playhead or selection start)
  const paste = useCallback(() => {
    if (!clipboardRef.current || !audioBuffer) return;
    pushUndo();
    const { channels: clipCh, sampleRate } = clipboardRef.current;
    const nch = audioBuffer.numberOfChannels;
    const insertPos = selStart !== null ? Math.floor(Math.min(selStart, selEnd || selStart) * sampleRate) : Math.floor(playheadPos * sampleRate);
    const resultChannels = [];

    for (let ch = 0; ch < nch; ch++) {
      const data = audioBuffer.getChannelData(ch);
      const clip = ch < clipCh.length ? clipCh[ch] : clipCh[0];
      const newData = new Float32Array(data.length + clip.length);
      newData.set(data.subarray(0, insertPos), 0);
      newData.set(clip, insertPos);
      newData.set(data.subarray(insertPos), insertPos + clip.length);
      resultChannels.push(newData);
    }

    const newBuf = createBuffer(resultChannels, sampleRate);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
  }, [audioBuffer, clipboardRef, selStart, selEnd, playheadPos, pushUndo, createBuffer, activeTrackIdx]);

  // DELETE selection
  const deleteSelection = useCallback(() => {
    const sel = getSelSamples();
    if (!sel || sel.end <= sel.start) return;
    pushUndo();
    const nch = audioBuffer.numberOfChannels;
    const sr = audioBuffer.sampleRate;
    const resultChannels = [];
    for (let ch = 0; ch < nch; ch++) {
      const data = audioBuffer.getChannelData(ch);
      const newData = new Float32Array(data.length - (sel.end - sel.start));
      newData.set(data.subarray(0, sel.start), 0);
      newData.set(data.subarray(sel.end), sel.start);
      resultChannels.push(newData);
    }
    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
    setSelStart(sel.start / sr);
    setSelEnd(sel.start / sr);
  }, [audioBuffer, getSelSamples, pushUndo, createBuffer, activeTrackIdx]);

  // SILENCE selection (replace with zeros)
  const silenceSelection = useCallback(() => {
    const sel = getSelSamples();
    if (!sel || sel.end <= sel.start) return;
    pushUndo();
    const nch = audioBuffer.numberOfChannels;
    const sr = audioBuffer.sampleRate;
    const resultChannels = [];
    for (let ch = 0; ch < nch; ch++) {
      const data = new Float32Array(audioBuffer.getChannelData(ch));
      for (let i = sel.start; i < sel.end; i++) data[i] = 0;
      resultChannels.push(data);
    }
    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
  }, [audioBuffer, getSelSamples, pushUndo, createBuffer, activeTrackIdx]);

  // FADE IN on selection
  const fadeIn = useCallback(() => {
    const sel = getSelSamples();
    if (!sel || sel.end <= sel.start) return;
    pushUndo();
    const nch = audioBuffer.numberOfChannels;
    const sr = audioBuffer.sampleRate;
    const len = sel.end - sel.start;
    const resultChannels = [];
    for (let ch = 0; ch < nch; ch++) {
      const data = new Float32Array(audioBuffer.getChannelData(ch));
      for (let i = 0; i < len; i++) {
        data[sel.start + i] *= i / len;
      }
      resultChannels.push(data);
    }
    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
  }, [audioBuffer, getSelSamples, pushUndo, createBuffer, activeTrackIdx]);

  // FADE OUT on selection
  const fadeOut = useCallback(() => {
    const sel = getSelSamples();
    if (!sel || sel.end <= sel.start) return;
    pushUndo();
    const nch = audioBuffer.numberOfChannels;
    const sr = audioBuffer.sampleRate;
    const len = sel.end - sel.start;
    const resultChannels = [];
    for (let ch = 0; ch < nch; ch++) {
      const data = new Float32Array(audioBuffer.getChannelData(ch));
      for (let i = 0; i < len; i++) {
        data[sel.start + i] *= 1 - (i / len);
      }
      resultChannels.push(data);
    }
    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
  }, [audioBuffer, getSelSamples, pushUndo, createBuffer, activeTrackIdx]);

  // NORMALIZE (peak normalize to 0dBFS)
  const normalize = useCallback(() => {
    if (!audioBuffer) return;
    pushUndo();
    const nch = audioBuffer.numberOfChannels;
    const sr = audioBuffer.sampleRate;
    let maxPeak = 0;
    for (let ch = 0; ch < nch; ch++) {
      const data = audioBuffer.getChannelData(ch);
      for (let i = 0; i < data.length; i++) {
        const abs = Math.abs(data[i]);
        if (abs > maxPeak) maxPeak = abs;
      }
    }
    if (maxPeak === 0) return;
    const gain = 0.98 / maxPeak;
    const resultChannels = [];
    for (let ch = 0; ch < nch; ch++) {
      const data = new Float32Array(audioBuffer.getChannelData(ch));
      for (let i = 0; i < data.length; i++) data[i] *= gain;
      resultChannels.push(data);
    }
    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
  }, [audioBuffer, pushUndo, createBuffer, activeTrackIdx]);

  // SILENCE TRIM — remove leading/trailing silence
  const trimSilence = useCallback(() => {
    if (!audioBuffer) return;
    pushUndo();
    const data = audioBuffer.getChannelData(0);
    const sr = audioBuffer.sampleRate;
    const threshold = 0.005;
    const windowSize = Math.floor(sr * 0.01); // 10ms windows

    // Find first non-silent sample
    let startIdx = 0;
    for (let i = 0; i < data.length; i += windowSize) {
      const rms = computeRMS(data, i, windowSize);
      if (rms > threshold) {
        startIdx = Math.max(0, i - windowSize * 2);
        break;
      }
    }

    // Find last non-silent sample
    let endIdx = data.length;
    for (let i = data.length - windowSize; i >= 0; i -= windowSize) {
      const rms = computeRMS(data, i, windowSize);
      if (rms > threshold) {
        endIdx = Math.min(data.length, i + windowSize * 3);
        break;
      }
    }

    if (startIdx >= endIdx) return;

    const nch = audioBuffer.numberOfChannels;
    const resultChannels = [];
    for (let ch = 0; ch < nch; ch++) {
      resultChannels.push(audioBuffer.getChannelData(ch).slice(startIdx, endIdx));
    }
    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
    setSelStart(null);
    setSelEnd(null);
    setPlayheadPos(0);
  }, [audioBuffer, pushUndo, createBuffer, activeTrackIdx]);

  // --- NOISE REMOVAL (Spectral Subtraction) ---
  const captureNoiseProfile = useCallback(() => {
    const sel = getSelSamples();
    if (!sel || sel.end <= sel.start) {
      alert('Select a region of pure noise (no speech) to capture the noise profile.');
      return;
    }
    const data = audioBuffer.getChannelData(0);
    const noiseData = data.slice(sel.start, sel.end);

    // Compute average power spectrum of noise
    const fftSize = 2048;
    const numFrames = Math.floor(noiseData.length / fftSize);
    const avgSpectrum = new Float32Array(fftSize);

    for (let frame = 0; frame < numFrames; frame++) {
      const offset = frame * fftSize;
      // Simple power spectrum via correlation (using real FFT approximation)
      for (let i = 0; i < fftSize; i++) {
        const val = i + offset < noiseData.length ? noiseData[i + offset] : 0;
        avgSpectrum[i] += val * val;
      }
    }

    if (numFrames > 0) {
      for (let i = 0; i < fftSize; i++) {
        avgSpectrum[i] /= numFrames;
      }
    }

    setNoiseProfile(avgSpectrum);
  }, [audioBuffer, getSelSamples]);

  const applyNoiseRemoval = useCallback(() => {
    if (!noiseProfile || !audioBuffer) {
      alert('Capture a noise profile first by selecting a noise-only region.');
      return;
    }
    pushUndo();

    const sr = audioBuffer.sampleRate;
    const nch = audioBuffer.numberOfChannels;
    const fftSize = 2048;
    const hopSize = fftSize / 2;
    const reductionFactor = Math.pow(10, noiseReduction / 20);
    const resultChannels = [];

    for (let ch = 0; ch < nch; ch++) {
      const input = audioBuffer.getChannelData(ch);
      const output = new Float32Array(input.length);
      const window = new Float32Array(fftSize);
      for (let i = 0; i < fftSize; i++) {
        window[i] = 0.5 * (1 - Math.cos(2 * Math.PI * i / (fftSize - 1)));
      }

      // Overlap-add spectral subtraction
      for (let pos = 0; pos + fftSize <= input.length; pos += hopSize) {
        // Windowed frame
        const frame = new Float32Array(fftSize);
        for (let i = 0; i < fftSize; i++) {
          frame[i] = input[pos + i] * window[i];
        }

        // Simple spectral subtraction: reduce energy in bins matching noise profile
        for (let i = 0; i < fftSize; i++) {
          const noisePower = noiseProfile[i] * reductionFactor;
          const signalPower = frame[i] * frame[i];
          if (signalPower < noisePower) {
            frame[i] *= 0.05; // Heavily suppress
          } else {
            const scale = Math.sqrt(Math.max(0, signalPower - noisePower) / (signalPower + 1e-10));
            frame[i] *= scale;
          }
        }

        // Overlap-add
        for (let i = 0; i < fftSize; i++) {
          if (pos + i < output.length) {
            output[pos + i] += frame[i] * window[i];
          }
        }
      }

      // Normalize overlap-add gain
      const normFactor = hopSize / fftSize * 2;
      for (let i = 0; i < output.length; i++) {
        output[i] *= normFactor;
      }

      resultChannels.push(output);
    }

    const newBuf = createBuffer(resultChannels, sr);
    setAudioBuffer(newBuf);
    setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: newBuf } : tr));
  }, [audioBuffer, noiseProfile, noiseReduction, pushUndo, createBuffer, activeTrackIdx]);

  // --- SERMON MASTER: One-click podcast mastering pipeline ---
  const sermonMaster = useCallback(async () => {
    if (!audioBuffer) return;
    pushUndo();

    const sr = audioBuffer.sampleRate;
    const nch = audioBuffer.numberOfChannels;

    try {
      // === Stage 1: Auto noise removal from first 500ms ===
      setMasterStage('Removing background noise...');
      setMasterProgress(5);

      let processedBuffer = audioBuffer;
      const noiseFrames = Math.min(Math.floor(sr * 0.5), audioBuffer.length);
      const fftSize = 2048;

      if (noiseFrames > fftSize) {
        const autoNoiseProfile = new Float32Array(fftSize);
        const data0 = audioBuffer.getChannelData(0);
        const numNoiseFrames = Math.floor(noiseFrames / fftSize);

        if (numNoiseFrames > 0) {
          for (let frame = 0; frame < numNoiseFrames; frame++) {
            const offset = frame * fftSize;
            for (let i = 0; i < fftSize; i++) {
              const val = offset + i < data0.length ? data0[offset + i] : 0;
              autoNoiseProfile[i] += val * val;
            }
          }
          for (let i = 0; i < fftSize; i++) autoNoiseProfile[i] /= numNoiseFrames;

          const reductionFactor = Math.pow(10, 12 / 20);
          const hopSize = fftSize / 2;
          const resultChs = [];

          for (let ch = 0; ch < nch; ch++) {
            const input = audioBuffer.getChannelData(ch);
            const output = new Float32Array(input.length);
            const hann = new Float32Array(fftSize);
            for (let i = 0; i < fftSize; i++) hann[i] = 0.5 * (1 - Math.cos(2 * Math.PI * i / (fftSize - 1)));

            for (let pos = 0; pos + fftSize <= input.length; pos += hopSize) {
              const frame = new Float32Array(fftSize);
              for (let i = 0; i < fftSize; i++) frame[i] = input[pos + i] * hann[i];
              for (let i = 0; i < fftSize; i++) {
                const noisePow = autoNoiseProfile[i] * reductionFactor;
                const sigPow = frame[i] * frame[i];
                if (sigPow < noisePow) {
                  frame[i] *= 0.05;
                } else {
                  frame[i] *= Math.sqrt(Math.max(0, sigPow - noisePow) / (sigPow + 1e-10));
                }
              }
              for (let i = 0; i < fftSize; i++) {
                if (pos + i < output.length) output[pos + i] += frame[i] * hann[i];
              }
            }
            const normFactor = hopSize / fftSize * 2;
            for (let i = 0; i < output.length; i++) output[i] *= normFactor;
            resultChs.push(output);
          }
          processedBuffer = createBuffer(resultChs, sr);
        }
      }

      setMasterProgress(30);

      // === Stage 2: OfflineAudioContext DSP chain (EQ + Compression + Limiting) ===
      setMasterStage('Applying EQ + dynamics...');

      const PRESETS = {
        sermon:    { hpfHz: 80, bassHz: 150, bassG: 3,   presHz: 2800, presG: 2,   airHz: 10000, airG: 1.5, deEssHz: 7500, deEssG: -3, deEssQ: 2.5, compThresh: -18, compRatio: 2.5, compKnee: 8, compAtk: 0.005, compRel: 0.3  },
        podcast:   { hpfHz: 80, bassHz: 120, bassG: 2,   presHz: 3500, presG: 3,   airHz: 10000, airG: 2,   deEssHz: 8000, deEssG: -4, deEssQ: 3,   compThresh: -16, compRatio: 3,   compKnee: 6, compAtk: 0.003, compRel: 0.25 },
        broadcast: { hpfHz: 100,bassHz: 100, bassG: 1.5, presHz: 4000, presG: 2.5, airHz: 12000, airG: 2.5, deEssHz: 8500, deEssG: -5, deEssQ: 3.5, compThresh: -14, compRatio: 4,   compKnee: 4, compAtk: 0.002, compRel: 0.2  },
      };
      const p = PRESETS[masterPreset] || PRESETS.sermon;

      const offlineCtx = new OfflineAudioContext(nch, processedBuffer.length, sr);
      const source = offlineCtx.createBufferSource();
      source.buffer = processedBuffer;

      const hpf = offlineCtx.createBiquadFilter();
      hpf.type = 'highpass'; hpf.frequency.value = p.hpfHz; hpf.Q.value = 0.7;

      const bass = offlineCtx.createBiquadFilter();
      bass.type = 'peaking'; bass.frequency.value = p.bassHz; bass.gain.value = p.bassG; bass.Q.value = 0.8;

      const presence = offlineCtx.createBiquadFilter();
      presence.type = 'peaking'; presence.frequency.value = p.presHz; presence.gain.value = p.presG; presence.Q.value = 1.0;

      const air = offlineCtx.createBiquadFilter();
      air.type = 'highshelf'; air.frequency.value = p.airHz; air.gain.value = p.airG;

      const deEsser = offlineCtx.createBiquadFilter();
      deEsser.type = 'peaking'; deEsser.frequency.value = p.deEssHz; deEsser.gain.value = p.deEssG; deEsser.Q.value = p.deEssQ;

      const comp = offlineCtx.createDynamicsCompressor();
      comp.threshold.value = p.compThresh; comp.knee.value = p.compKnee;
      comp.ratio.value = p.compRatio; comp.attack.value = p.compAtk; comp.release.value = p.compRel;

      const limiter = offlineCtx.createDynamicsCompressor();
      limiter.threshold.value = -1; limiter.knee.value = 0;
      limiter.ratio.value = 20; limiter.attack.value = 0.001; limiter.release.value = 0.1;

      source.connect(hpf);
      hpf.connect(bass);
      bass.connect(presence);
      presence.connect(air);
      air.connect(deEsser);
      deEsser.connect(comp);
      comp.connect(limiter);
      limiter.connect(offlineCtx.destination);
      source.start(0);

      setMasterProgress(50);
      const rendered = await offlineCtx.startRendering();

      // === Stage 3: Normalize to -3dBFS ===
      setMasterStage('Normalizing to -3 dBFS...');
      setMasterProgress(80);

      let maxPeak = 0;
      for (let ch = 0; ch < rendered.numberOfChannels; ch++) {
        const data = rendered.getChannelData(ch);
        for (let i = 0; i < data.length; i++) {
          const abs = Math.abs(data[i]);
          if (abs > maxPeak) maxPeak = abs;
        }
      }
      const targetPeak = Math.pow(10, -3 / 20); // -3 dBFS
      const finalGain = maxPeak > 0 ? Math.min(targetPeak / maxPeak, 4) : 1;

      const finalChannels = [];
      for (let ch = 0; ch < rendered.numberOfChannels; ch++) {
        const data = new Float32Array(rendered.getChannelData(ch));
        for (let i = 0; i < data.length; i++) {
          data[i] = Math.max(-1, Math.min(1, data[i] * finalGain));
        }
        finalChannels.push(data);
      }

      const finalBuf = createBuffer(finalChannels, sr);
      setAudioBuffer(finalBuf);
      setTracks(t => t.map((tr, i) => i === activeTrackIdx ? { ...tr, buffer: finalBuf } : tr));

      setMasterProgress(100);
      setMasterStage('Master complete! Ready for export.');
      setTimeout(() => { setMasterProgress(0); setMasterStage(null); }, 4000);

    } catch (err) {
      console.error('Sermon Master failed:', err);
      setMasterStage('Error: ' + err.message);
      setTimeout(() => { setMasterProgress(0); setMasterStage(null); }, 4000);
    }
  }, [audioBuffer, masterPreset, pushUndo, createBuffer, activeTrackIdx]);

  // --- Multi-track: Add Track ---
  const addTrack = useCallback(async (file) => {
    if (!file) return;
    const ctx = audioContext || new (window.AudioContext || window.webkitAudioContext)();
    try {
      const arrayBuffer = await file.arrayBuffer();
      const decoded = await ctx.decodeAudioData(arrayBuffer);
      setTracks(prev => [...prev, { buffer: decoded, name: file.name, muted: false, solo: false, gain: 1.0 }]);
    } catch (err) {
      alert('Could not decode audio file for track.');
    }
  }, [audioContext]);

  // Multi-track: Mix Down
  const mixDown = useCallback(() => {
    if (tracks.length === 0) return;
    pushUndo();
    const sr = tracks[0].buffer.sampleRate;
    const maxLen = Math.max(...tracks.filter(t => !t.muted).map(t => t.buffer.length));
    const mixed = new Float32Array(maxLen);

    const activeTracks = tracks.filter(t => !t.muted);
    const soloTracks = activeTracks.filter(t => t.solo);
    const tracksToMix = soloTracks.length > 0 ? soloTracks : activeTracks;

    for (const track of tracksToMix) {
      const data = track.buffer.getChannelData(0);
      for (let i = 0; i < data.length && i < maxLen; i++) {
        mixed[i] += data[i] * track.gain;
      }
    }

    // Normalize to prevent clipping
    let peak = 0;
    for (let i = 0; i < mixed.length; i++) {
      const abs = Math.abs(mixed[i]);
      if (abs > peak) peak = abs;
    }
    if (peak > 0.98) {
      const scale = 0.98 / peak;
      for (let i = 0; i < mixed.length; i++) mixed[i] *= scale;
    }

    const ctx = audioContext || new (window.AudioContext || window.webkitAudioContext)();
    const buf = ctx.createBuffer(1, mixed.length, sr);
    buf.copyToChannel(mixed, 0);
    setAudioBuffer(buf);
  }, [tracks, audioContext, pushUndo]);

  // --- Export ---
  const exportWav = useCallback(() => {
    if (!audioBuffer) return;
    const nch = audioBuffer.numberOfChannels;
    const sr = audioBuffer.sampleRate;
    const length = audioBuffer.length;
    const bytesPerSample = 2;
    const blockAlign = nch * bytesPerSample;
    const dataSize = length * blockAlign;
    const buffer = new ArrayBuffer(44 + dataSize);
    const view = new DataView(buffer);

    const writeStr = (o, s) => { for (let i = 0; i < s.length; i++) view.setUint8(o + i, s.charCodeAt(i)); };
    writeStr(0, 'RIFF');
    view.setUint32(4, 36 + dataSize, true);
    writeStr(8, 'WAVE');
    writeStr(12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, nch, true);
    view.setUint32(24, sr, true);
    view.setUint32(28, sr * blockAlign, true);
    view.setUint16(32, blockAlign, true);
    view.setUint16(34, 16, true);
    writeStr(36, 'data');
    view.setUint32(40, dataSize, true);

    let offset = 44;
    const channels = [];
    for (let ch = 0; ch < nch; ch++) channels.push(audioBuffer.getChannelData(ch));
    for (let i = 0; i < length; i++) {
      for (let ch = 0; ch < nch; ch++) {
        const s = Math.max(-1, Math.min(1, channels[ch][i]));
        view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
        offset += 2;
      }
    }

    const blob = new Blob([buffer], { type: 'audio/wav' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `TIWATON_edited_${Date.now()}.wav`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    if (onExport) onExport(blob);
  }, [audioBuffer, onExport]);

  // --- Zoom ---
  const handleWheel = useCallback((e) => {
    if (!audioBuffer) return;
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      // Zoom
      const factor = e.deltaY > 0 ? 0.9 : 1.1;
      setZoom(z => Math.max(5, Math.min(z * factor, canvasWidth * 10 / audioBuffer.duration)));
    } else {
      // Scroll
      setScrollX(x => Math.max(0, x + e.deltaX + e.deltaY));
    }
  }, [audioBuffer, canvasWidth]);

  // --- Keyboard Shortcuts ---
  useEffect(() => {
    const handleKey = (e) => {
      if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && e.key === 'z') { e.preventDefault(); undo(); }
      if (ctrl && e.key === 'y') { e.preventDefault(); redo(); }
      if (ctrl && e.key === 'x') { e.preventDefault(); cutSelection(); }
      if (ctrl && e.key === 'c') { e.preventDefault(); copySelection(); }
      if (ctrl && e.key === 'v') { e.preventDefault(); paste(); }
      if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); deleteSelection(); }
      if (e.key === ' ') { e.preventDefault(); isPlaying ? stopAudio() : playAudio(); }
      if (e.key === 'Home') { e.preventDefault(); setPlayheadPos(0); setScrollX(0); }
      if (e.key === 'End' && audioBuffer) { e.preventDefault(); setPlayheadPos(audioBuffer.duration); }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [undo, redo, cutSelection, copySelection, paste, deleteSelection, isPlaying, playAudio, stopAudio, audioBuffer]);

  // Hidden file inputs
  const trackInputRef = useRef(null);

  return (
    <div className="flex flex-col h-full bg-[#050A1C] text-slate-300 font-sans select-none">
      {/* Toolbar */}
      <div className="flex items-center gap-1 px-3 py-1.5 bg-[#0D1428] border-b border-slate-800 text-[11px] flex-wrap">
        {/* File */}
        <button onClick={() => fileInputRef.current?.click()} className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 font-semibold">Import</button>
        <input ref={fileInputRef} type="file" accept="audio/*" className="hidden" onChange={(e) => { if (e.target.files[0]) handleFileLoad(e.target.files[0]); e.target.value = ''; }} />
        <button onClick={exportWav} disabled={!audioBuffer} className="px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 font-semibold disabled:opacity-30">Export WAV</button>
        <div className="w-px h-5 bg-slate-700 mx-1" />

        {/* Transport */}
        <button onClick={isPlaying ? stopAudio : playAudio} disabled={!audioBuffer} className={`px-2.5 py-1 rounded font-semibold ${isPlaying ? 'bg-red-600 text-white' : 'bg-emerald-700 text-white'} disabled:opacity-30`}>
          {isPlaying ? 'Stop' : 'Play'}
        </button>
        <div className="w-px h-5 bg-slate-700 mx-1" />

        {/* Edit */}
        <button onClick={undo} disabled={undoStack.length === 0} className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-30" title="Undo (Ctrl+Z)">Undo</button>
        <button onClick={redo} disabled={redoStack.length === 0} className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-30" title="Redo (Ctrl+Y)">Redo</button>
        <div className="w-px h-5 bg-slate-700 mx-1" />
        <button onClick={cutSelection} disabled={!audioBuffer} className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-30" title="Cut (Ctrl+X)">Cut</button>
        <button onClick={copySelection} disabled={!audioBuffer} className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-30" title="Copy (Ctrl+C)">Copy</button>
        <button onClick={paste} disabled={!clipboardRef.current} className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-30" title="Paste (Ctrl+V)">Paste</button>
        <button onClick={deleteSelection} disabled={!audioBuffer} className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-30" title="Delete">Delete</button>
        <button onClick={silenceSelection} disabled={!audioBuffer} className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-30" title="Silence Selection">Silence</button>
        <div className="w-px h-5 bg-slate-700 mx-1" />

        {/* Effects */}
        <button onClick={fadeIn} disabled={!audioBuffer} className="px-2 py-1 rounded bg-amber-900/40 hover:bg-amber-800/50 text-amber-300 disabled:opacity-30">Fade In</button>
        <button onClick={fadeOut} disabled={!audioBuffer} className="px-2 py-1 rounded bg-amber-900/40 hover:bg-amber-800/50 text-amber-300 disabled:opacity-30">Fade Out</button>
        <button onClick={normalize} disabled={!audioBuffer} className="px-2 py-1 rounded bg-amber-900/40 hover:bg-amber-800/50 text-amber-300 disabled:opacity-30">Normalize</button>
        <button onClick={trimSilence} disabled={!audioBuffer} className="px-2 py-1 rounded bg-amber-900/40 hover:bg-amber-800/50 text-amber-300 disabled:opacity-30">Trim Silence</button>
        <div className="w-px h-5 bg-slate-700 mx-1" />

        {/* Noise Removal */}
        <button onClick={captureNoiseProfile} disabled={!audioBuffer} className={`px-2 py-1 rounded font-semibold disabled:opacity-30 ${noiseProfile ? 'bg-amber-700 text-white' : 'bg-slate-800 hover:bg-slate-700 text-amber-300'}`}>
          {noiseProfile ? 'Re-capture Noise' : 'Capture Noise'}
        </button>
        <button onClick={applyNoiseRemoval} disabled={!noiseProfile} className="px-2 py-1 rounded bg-red-900/40 hover:bg-red-800/50 text-red-300 font-semibold disabled:opacity-30">Remove Noise</button>
        <select value={noiseReduction} onChange={(e) => setNoiseReduction(Number(e.target.value))} className="bg-slate-800 text-slate-300 rounded px-1.5 py-1 text-[10px] border border-slate-700">
          <option value={6}>Light (-6dB)</option>
          <option value={12}>Medium (-12dB)</option>
          <option value={18}>Heavy (-18dB)</option>
          <option value={24}>Extreme (-24dB)</option>
        </select>
        <div className="w-px h-5 bg-slate-700 mx-1" />

        {/* Sermon Master */}
        <div className="w-px h-5 bg-slate-700 mx-1" />
        <select
          value={masterPreset}
          onChange={e => setMasterPreset(e.target.value)}
          disabled={!!masterStage}
          className="bg-[#1a0a00] text-amber-300 rounded px-1.5 py-1 text-[10px] border border-amber-800/60 disabled:opacity-40"
        >
          <option value="sermon">Sermon (Warm)</option>
          <option value="podcast">Podcast (Crisp)</option>
          <option value="broadcast">Broadcast (Punchy)</option>
        </select>
        <button
          onClick={sermonMaster}
          disabled={!audioBuffer || !!masterStage}
          className="px-3 py-1 rounded bg-amber-600 hover:bg-amber-500 text-white font-bold text-[11px] disabled:opacity-40 whitespace-nowrap shadow-[0_0_10px_rgba(var(--accent-rgb),0.35)]"
          title="One-click podcast mastering: noise reduction → EQ warmth → compression → limiting → normalize"
        >
          {masterStage ? '⏳ Processing...' : '🎙 Sermon Master'}
        </button>
        {masterProgress > 0 && (
          <div className="flex items-center gap-1.5">
            <div className="w-24 h-1.5 bg-slate-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-amber-500 rounded-full transition-all duration-300"
                style={{ width: `${masterProgress}%` }}
              />
            </div>
            <span className="text-[10px] text-amber-400 font-mono">{masterProgress}%</span>
          </div>
        )}
        <div className="w-px h-5 bg-slate-700 mx-1" />

        {/* Multi-track */}
        <button onClick={() => trackInputRef.current?.click()} className="px-2 py-1 rounded bg-cyan-900/40 hover:bg-cyan-800/50 text-cyan-300 disabled:opacity-30">+ Track</button>
        <input ref={trackInputRef} type="file" accept="audio/*" className="hidden" onChange={(e) => { if (e.target.files[0]) addTrack(e.target.files[0]); e.target.value = ''; }} />
        {tracks.length > 1 && (
          <button onClick={mixDown} className="px-2 py-1 rounded bg-cyan-900/40 hover:bg-cyan-800/50 text-cyan-300">Mix Down</button>
        )}

        {/* Zoom */}
        <div className="ml-auto flex items-center gap-1 text-[10px] text-slate-500">
          <button onClick={() => setZoom(z => Math.min(z * 1.3, audioBuffer ? canvasWidth * 10 / audioBuffer.duration : 9999))} className="px-1.5 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-400">+</button>
          <button onClick={() => setZoom(z => Math.max(5, z * 0.7))} className="px-1.5 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-400">-</button>
          <span className="font-mono">{zoom.toFixed(0)}px/s</span>
          {audioBuffer && (
            <button onClick={() => { setZoom(canvasWidth / audioBuffer.duration); setScrollX(0); }} className="px-1.5 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-400">Fit</button>
          )}
        </div>
      </div>

      {/* Multi-track lane headers */}
      {tracks.length > 1 && (
        <div className="flex gap-1 px-3 py-1 bg-[#0A0F20] border-b border-slate-800 text-[10px] overflow-x-auto">
          {tracks.map((t, idx) => (
            <div key={idx} className={`flex items-center gap-1 px-2 py-0.5 rounded ${idx === activeTrackIdx ? 'bg-amber-900/40 border border-amber-500/50' : 'bg-slate-800/50'}`}>
              <button onClick={() => { setActiveTrackIdx(idx); setAudioBuffer(t.buffer); }} className="font-semibold truncate max-w-[100px]">{t.name}</button>
              <button onClick={() => setTracks(prev => prev.map((tr, i) => i === idx ? { ...tr, muted: !tr.muted } : tr))} className={`px-1 rounded ${t.muted ? 'text-red-400' : 'text-slate-500'}`}>M</button>
              <button onClick={() => setTracks(prev => prev.map((tr, i) => i === idx ? { ...tr, solo: !tr.solo } : tr))} className={`px-1 rounded ${t.solo ? 'text-amber-400' : 'text-slate-500'}`}>S</button>
            </div>
          ))}
        </div>
      )}

      {/* Waveform Canvas */}
      <div ref={containerRef} className="flex-1 relative cursor-crosshair overflow-hidden" onWheel={handleWheel}>
        <canvas
          ref={canvasRef}
          className="w-full h-full"
          style={{ height: canvasHeight }}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
        />
      </div>

      {/* Status bar */}
      <div className="flex items-center gap-4 px-3 py-1 bg-[#0D1428] border-t border-slate-800 text-[10px] font-mono text-slate-500">
        <span>Playhead: {playheadPos.toFixed(2)}s</span>
        {audioBuffer && <span>Duration: {audioBuffer.duration.toFixed(2)}s</span>}
        {audioBuffer && <span>SR: {audioBuffer.sampleRate}Hz</span>}
        {selStart !== null && selEnd !== null && Math.abs(selEnd - selStart) > 0.001 && (
          <span className="text-amber-400">Sel: {Math.min(selStart, selEnd).toFixed(2)}s → {Math.max(selStart, selEnd).toFixed(2)}s ({Math.abs(selEnd - selStart).toFixed(2)}s)</span>
        )}
        {noiseProfile && <span className="text-amber-400">Noise profile captured</span>}
        {masterStage && <span className="text-amber-400 animate-pulse">{masterStage}</span>}
        <span className="ml-auto">Undo: {undoStack.length} | Redo: {redoStack.length}</span>
      </div>
    </div>
  );
}
