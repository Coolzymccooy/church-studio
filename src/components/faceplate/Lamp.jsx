const TONE = { g: 'fp-led-g', a: 'fp-led-a', r: 'fp-led-r', off: '' };

/** A status lamp: an LED plus a word. State is always in the word, never colour alone. */
export default function Lamp({ tone = 'off', children, className = '' }) {
  return (
    <span className={['fp-lamp', className].filter(Boolean).join(' ')}>
      <span className={['fp-led', TONE[tone]].filter(Boolean).join(' ')} aria-hidden="true" />
      {children}
    </span>
  );
}
