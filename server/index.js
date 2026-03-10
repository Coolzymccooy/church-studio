import express from 'express';
import cors from 'cors';
import multer from 'multer';
import ffmpeg from 'fluent-ffmpeg';
import { createReadStream, createWriteStream, mkdirSync, existsSync, readFileSync, writeFileSync, unlinkSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { randomUUID } from 'crypto';

const __dirname = dirname(fileURLToPath(import.meta.url));

const app = express();
const PORT = process.env.PORT || 3333;

app.use(cors());
app.use(express.json());

// --- Storage ---
const uploadsDir = join(__dirname, 'uploads');
const processedDir = join(__dirname, 'processed');
const profilesFile = join(__dirname, 'noise-profiles.json');

[uploadsDir, processedDir].forEach(dir => {
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
});

// --- Multer for file uploads ---
const upload = multer({
  dest: uploadsDir,
  limits: { fileSize: 500 * 1024 * 1024 }, // 500MB max
  fileFilter: (req, file, cb) => {
    const allowed = ['audio/wav', 'audio/mpeg', 'audio/mp3', 'audio/webm', 'audio/ogg', 'audio/flac', 'audio/mp4', 'audio/x-wav'];
    if (allowed.includes(file.mimetype) || file.originalname.match(/\.(wav|mp3|webm|ogg|flac|m4a|aac)$/i)) {
      cb(null, true);
    } else {
      cb(new Error(`Unsupported audio format: ${file.mimetype}`));
    }
  },
});

// ============================================================
// NOISE PROFILES DATABASE
// ============================================================

function loadProfiles() {
  if (existsSync(profilesFile)) {
    try {
      return JSON.parse(readFileSync(profilesFile, 'utf-8'));
    } catch {
      return { rooms: {} };
    }
  }
  return { rooms: {} };
}

function saveProfiles(data) {
  writeFileSync(profilesFile, JSON.stringify(data, null, 2));
}

// GET all profiles
app.get('/api/profiles', (req, res) => {
  const data = loadProfiles();
  res.json(data);
});

// GET profiles for a specific room
app.get('/api/profiles/:roomName', (req, res) => {
  const data = loadProfiles();
  const room = data.rooms[req.params.roomName];
  if (!room) return res.status(404).json({ error: 'Room not found' });
  res.json(room);
});

// POST create/update a noise profile
app.post('/api/profiles', (req, res) => {
  const { roomName, profileName, lowReduction, highReduction, threshold, spectrum } = req.body;

  if (!roomName || !profileName) {
    return res.status(400).json({ error: 'roomName and profileName are required' });
  }

  const data = loadProfiles();
  if (!data.rooms[roomName]) {
    data.rooms[roomName] = { profiles: [] };
  }

  const profile = {
    id: randomUUID(),
    name: profileName,
    lowReduction: lowReduction || 0,
    highReduction: highReduction || 0,
    threshold: threshold || -50,
    spectrum: spectrum || null,
    createdAt: new Date().toISOString(),
  };

  data.rooms[roomName].profiles.push(profile);
  saveProfiles(data);

  res.json({ success: true, profile });
});

// DELETE a profile
app.delete('/api/profiles/:roomName/:profileId', (req, res) => {
  const data = loadProfiles();
  const room = data.rooms[req.params.roomName];
  if (!room) return res.status(404).json({ error: 'Room not found' });

  room.profiles = room.profiles.filter(p => p.id !== req.params.profileId);
  saveProfiles(data);

  res.json({ success: true });
});

// ============================================================
// AUDIO PROCESSING (ffmpeg pipeline)
// ============================================================

// Processing presets matching the frontend AI stack
const processingPresets = {
  // Church Beta: full processing chain
  'church-beta': {
    filters: [
      // High-pass (remove rumble below 80Hz)
      'highpass=f=80',
      // Noise gate: attack 10ms, release 250ms
      'agate=threshold=0.01:ratio=8:attack=10:release=250',
      // De-hum (notch at 50Hz and 60Hz for power line)
      'bandreject=f=50:width_type=q:w=5',
      'bandreject=f=60:width_type=q:w=5',
      // Compressor (gentle speech leveling)
      'acompressor=threshold=-30dB:ratio=4:attack=5:release=200:makeup=6',
      // De-esser (reduce sibilance 5-8kHz)
      'equalizer=f=6500:width_type=q:w=2:g=-4',
      // Warmth (boost low-mids)
      'equalizer=f=250:width_type=q:w=1.1:g=3.5',
      // Clarity (boost presence)
      'equalizer=f=3500:width_type=q:w=1.2:g=4',
      // Air (subtle high-shelf shine)
      'treble=g=2.5:f=9000',
      // Limiter (brick wall at -3dB)
      'alimiter=limit=0.7:attack=0.5:release=50',
      // Loudness normalization to -16 LUFS (YouTube standard)
      'loudnorm=I=-16:TP=-1.5:LRA=11',
    ],
  },

  // Light cleanup only
  'clean': {
    filters: [
      'highpass=f=80',
      'agate=threshold=0.008:ratio=6:attack=10:release=200',
      'bandreject=f=50:width_type=q:w=5',
      'bandreject=f=60:width_type=q:w=5',
      'loudnorm=I=-16:TP=-1.5:LRA=11',
    ],
  },

  // Sermon recording (warm, compressed, polished)
  'sermon': {
    filters: [
      'highpass=f=80',
      'agate=threshold=0.01:ratio=8:attack=10:release=250',
      'bandreject=f=50:width_type=q:w=5',
      'bandreject=f=60:width_type=q:w=5',
      'acompressor=threshold=-28dB:ratio=5:attack=5:release=180:makeup=8',
      'equalizer=f=6500:width_type=q:w=2:g=-3',
      'equalizer=f=250:width_type=q:w=1.1:g=4',
      'equalizer=f=3500:width_type=q:w=1.2:g=5',
      'equalizer=f=1500:width_type=q:w=1:g=2',
      'treble=g=3:f=9000',
      'alimiter=limit=0.7:attack=0.5:release=50',
      'loudnorm=I=-16:TP=-1.5:LRA=11',
    ],
  },

  // Livestream safe (conservative, no clipping)
  'livestream': {
    filters: [
      'highpass=f=80',
      'agate=threshold=0.01:ratio=6:attack=10:release=300',
      'bandreject=f=50:width_type=q:w=5',
      'bandreject=f=60:width_type=q:w=5',
      'acompressor=threshold=-25dB:ratio=3:attack=10:release=250:makeup=4',
      'equalizer=f=250:width_type=q:w=1:g=2',
      'equalizer=f=3500:width_type=q:w=1:g=3',
      'alimiter=limit=0.8:attack=1:release=80',
      'loudnorm=I=-14:TP=-1:LRA=9',
    ],
  },
};

// POST /api/process - Upload and process audio file
app.post('/api/process', upload.single('audio'), async (req, res) => {
  if (!req.file) {
    return res.status(400).json({ error: 'No audio file provided' });
  }

  const preset = req.body.preset || 'church-beta';
  const presetConfig = processingPresets[preset];

  if (!presetConfig) {
    unlinkSync(req.file.path);
    return res.status(400).json({
      error: `Unknown preset: ${preset}`,
      available: Object.keys(processingPresets),
    });
  }

  const outputFilename = `processed_${Date.now()}_${req.file.originalname || 'audio'}.wav`;
  const outputPath = join(processedDir, outputFilename);

  try {
    await new Promise((resolve, reject) => {
      let cmd = ffmpeg(req.file.path)
        .audioCodec('pcm_s24le')
        .audioFrequency(48000)
        .audioChannels(1);

      // Apply filter chain
      const filterString = presetConfig.filters.join(',');
      cmd = cmd.audioFilters(filterString);

      cmd
        .output(outputPath)
        .on('end', resolve)
        .on('error', reject)
        .run();
    });

    // Clean up upload
    unlinkSync(req.file.path);

    // Send processed file
    res.download(outputPath, outputFilename, (err) => {
      // Clean up after download
      try { unlinkSync(outputPath); } catch {}
    });
  } catch (err) {
    try { unlinkSync(req.file.path); } catch {}
    try { unlinkSync(outputPath); } catch {}
    res.status(500).json({ error: 'Processing failed', details: err.message });
  }
});

// POST /api/process-stream - Process audio stream (for live recording)
app.post('/api/process-stream', upload.single('audio'), async (req, res) => {
  if (!req.file) {
    return res.status(400).json({ error: 'No audio file provided' });
  }

  const preset = req.body.preset || 'church-beta';
  const presetConfig = processingPresets[preset];

  if (!presetConfig) {
    unlinkSync(req.file.path);
    return res.status(400).json({ error: `Unknown preset: ${preset}` });
  }

  try {
    res.setHeader('Content-Type', 'audio/wav');
    res.setHeader('Content-Disposition', 'attachment; filename="processed.wav"');

    const filterString = presetConfig.filters.join(',');

    ffmpeg(req.file.path)
      .audioCodec('pcm_s24le')
      .audioFrequency(48000)
      .audioChannels(1)
      .audioFilters(filterString)
      .format('wav')
      .pipe(res, { end: true })
      .on('end', () => {
        try { unlinkSync(req.file.path); } catch {}
      })
      .on('error', () => {
        try { unlinkSync(req.file.path); } catch {}
      });
  } catch (err) {
    try { unlinkSync(req.file.path); } catch {}
    res.status(500).json({ error: 'Stream processing failed', details: err.message });
  }
});

// GET /api/presets - List available processing presets
app.get('/api/presets', (req, res) => {
  res.json({
    presets: Object.entries(processingPresets).map(([name, config]) => ({
      name,
      filterCount: config.filters.length,
      filters: config.filters,
    })),
  });
});

// Health check
app.get('/api/health', (req, res) => {
  res.json({
    status: 'ok',
    service: 'TIWATON Church Studio Server',
    version: '1.0.0',
    ffmpeg: 'required',
  });
});

app.listen(PORT, () => {
  console.log(`\n  TIWATON Church Studio Server`);
  console.log(`  ----------------------------`);
  console.log(`  API:      http://localhost:${PORT}`);
  console.log(`  Health:   http://localhost:${PORT}/api/health`);
  console.log(`  Profiles: http://localhost:${PORT}/api/profiles`);
  console.log(`  Presets:  http://localhost:${PORT}/api/presets`);
  console.log(`\n  Requires: ffmpeg installed and in PATH\n`);
});
