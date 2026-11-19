/** A recessed lit window for anything measured or live: readouts, meters, logs. */
export default function Glass({ as = 'div', className = '', children, ...rest }) {
  const Tag = as;
  return (
    <Tag className={['fp-glass', className].filter(Boolean).join(' ')} {...rest}>
      {children}
    </Tag>
  );
}
