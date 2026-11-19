/** Help drawer content: quick fixes, the Sunday setup guide and the pre-service checklist. */

export const QUICK_FIXES = [
  {
    title: 'No sound on stream',
    content: 'Make sure LIVE is ON. In OBS choose VB-CABLE Input or BlackHole, not your physical microphone.',
  },
  {
    title: 'Too much background noise / HVAC hum',
    content: 'Enable Hyper-Gate and RNNoise, then run Auto-Calibrate Gate while the room is at its normal noise level.',
  },
  {
    title: 'Echo / hearing voice twice',
    content: 'Remove every OBS audio source except the TIWATON virtual cable. Do not send both raw mic and processed audio.',
  },
  {
    title: 'Voice getting cut off mid-sentence',
    content: 'Lower the gate threshold or run Auto-Calibrate again. If needed, disable Hyper-Gate temporarily.',
  },
  {
    title: 'Audio sounds thin or tinny',
    content: 'Enable Sermon Warmth and Voice Polish. In the editor, use the Sermon Warm preset before exporting WAV.',
  },
  {
    title: 'Clipping / levels too hot',
    content: 'Reduce gain on the mixer or interface first, then lower input gain in TIWATON.',
  },
  {
    title: 'App crashed or frozen',
    content: 'Click Reset, wait a few seconds, then start Live again. Avoid changing devices while the engine is live.',
  },
  {
    title: 'Making a sermon recording sound podcast-quality',
    content: 'Import the recording in the Audio Editor, choose a mastering preset, run Sermon Master, then export WAV.',
  },
];

export const SETUP_STEPS = [
  {
    num: '1',
    title: 'Physical Connection',
    items: [
      'Pastor mic to mixer or audio interface, not directly into the laptop',
      'One clean USB cable from interface into computer',
      'Disable OS noise suppression and AGC',
      'Monitor on headphones, not speakers',
    ],
  },
  {
    num: '2',
    title: 'Inside TIWATON',
    items: [
      'Select the interface as the input source',
      'Set Broadcast Output to VB-CABLE Input or BlackHole',
      'Enable Hyper-Gate, Voice Isolation, Warmth, and Auto-Mix',
      'Run Auto-Calibrate Gate with normal room noise',
    ],
  },
  {
    num: '3',
    title: 'In OBS / StreamYard',
    items: [
      'Choose the TIWATON broadcast device as Audio Input Capture',
      'Remove physical mic and webcam sources',
      'Use the same broadcast device in both TIWATON and OBS',
      'Confirm OBS meters move when the pastor speaks',
    ],
  },
];

export const CHECKLIST = [
  { step: 'Mic plugged into mixer or USB interface', group: 'Before You Start' },
  { step: 'Gain set correctly — no red clipping on input', group: 'Before You Start' },
  { step: 'System mic enhancements turned OFF', group: 'Before You Start' },
  { step: 'TIWATON input source set to your interface', group: 'TIWATON Setup' },
  { step: 'Broadcast Output set to VB-CABLE or BlackHole', group: 'TIWATON Setup' },
  { step: 'Hyper-Gate, Voice Isolation, Warmth, Auto-Mix all enabled', group: 'TIWATON Setup' },
  { step: 'Auto-Calibrate Gate run with normal room noise', group: 'TIWATON Setup' },
  { step: 'OBS audio source set to the same virtual cable', group: 'OBS / Streaming' },
  { step: 'All raw microphone inputs removed from OBS', group: 'OBS / Streaming' },
  { step: '10-second test recording sounds clear', group: 'Sound Check' },
  { step: 'Press LIVE — status shows green Live Processing', group: 'Go Live' },
];
