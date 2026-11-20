export function keepHeldFaders(current, incoming, held) {
  if (!current || !incoming || held.size === 0) return incoming;
  const preserve = (next, previous, kind, key) => {
    const values = new Map(previous.map((item) => [item[key], item.fader_db]));
    return next.map((item) => {
      const id = item[key];
      return held.has(`${kind}:${id}`) && values.has(id)
        ? { ...item, fader_db: values.get(id) }
        : item;
    });
  };
  return {
    ...incoming,
    strips: preserve(incoming.strips, current.strips, 'strip', 'index'),
    buses: preserve(incoming.buses, current.buses, 'bus', 'id'),
  };
}
