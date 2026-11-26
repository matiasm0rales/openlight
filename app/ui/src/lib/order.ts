// El orden de los focos: la parte que no depende de la pantalla.

/** La lista con el elemento de `from` puesto en el lugar `to`. Fuera de rango, igual que estaba. */
export function move<T>(list: T[], from: number, to: number): T[] {
  const next = [...list];
  if (from < 0 || to < 0 || from >= list.length || to >= list.length) return next;
  next.splice(to, 0, ...next.splice(from, 1));
  return next;
}

/** Los elementos en el orden de `ids`. Lo que no figura en `ids` queda al final, como estaba. */
export function sortByIds<T extends { id: string }>(items: T[], ids: string[]): T[] {
  const place = new Map(ids.map((id, i) => [id, i]));
  return [...items].sort((a, b) => (place.get(a.id) ?? ids.length) - (place.get(b.id) ?? ids.length));
}

/** Dónde está dibujada una tarjeta (las medidas de `getBoundingClientRect`). */
export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Cuánto puede alejarse el puntero de una tarjeta y seguir apuntándole (cubre la separación). */
const REACH = 40;

/**
 * El lugar que tomaría el elemento `from` si se lo suelta en (`x`, `y`): el de la tarjeta que
 * está ahí (o la más cercana). `null` si soltarlo no cambia nada: su propio lugar, o lejos.
 */
export function dropAt(boxes: Box[], from: number, x: number, y: number): number | null {
  let over: number | null = null;
  let nearest = REACH;
  for (const [i, box] of boxes.entries()) {
    const dx = Math.max(box.left - x, 0, x - box.left - box.width);
    const dy = Math.max(box.top - y, 0, y - box.top - box.height);
    const distance = Math.hypot(dx, dy);
    if (distance <= nearest) [over, nearest] = [i, distance];
  }
  return over === from ? null : over;
}
