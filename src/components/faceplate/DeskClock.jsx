import { useEffect, useState } from 'react';
import { formatClock } from '../../lib/faceplateHeader.js';
import Glass from './Glass.jsx';

/** Wall clock in the header. Ticks on its own so the rest of the header does not re-render each second. */
export default function DeskClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  return (
    <Glass as="time" className="fp-clock" dateTime={now.toISOString()} aria-label="Time">
      {formatClock(now)}
    </Glass>
  );
}
