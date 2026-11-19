const LIT = {
  green: 'fp-on-green', amber: 'fp-on-amber', orange: 'fp-on-orange', sel: 'fp-on-sel', red: 'fp-on-red',
};

/** A faceplate key. `lit` picks the lit colour by meaning; `pressed` marks a toggle key. */
export default function Key({ lit = null, pressed, className = '', children, ...rest }) {
  const cls = ['fp-key', lit && LIT[lit], className].filter(Boolean).join(' ');
  return (
    <button type="button" className={cls} aria-pressed={pressed} {...rest}>
      {children}
    </button>
  );
}
