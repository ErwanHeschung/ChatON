/**
 * Equal Earth, as `scripts/world.py` draws the map: [longitude, latitude] in degrees to SVG
 * units (the world is 1000 wide). Keep the two in step.
 */
const [A1, A2, A3, A4] = [1.340264, -0.081106, 0.000893, 0.003796];
const M = Math.sqrt(3) / 2;
const rad = Math.PI / 180;

function equalEarth(lon: number, lat: number): [number, number] {
  const t = Math.asin(M * Math.sin(lat * rad));
  const [t2, t6] = [t * t, t ** 6];
  const x =
    (2 * Math.sqrt(3) * lon * rad * Math.cos(t)) /
    (3 * (A1 + 3 * A2 * t2 + t6 * (7 * A3 + 9 * A4 * t2)));
  return [x, t * (A1 + A2 * t2 + t6 * (A3 + A4 * t2))];
}

const XMAX = equalEarth(180, 0)[0];
const YMAX = equalEarth(0, 90)[1];
const R = 1000 / (2 * XMAX);

export function project(lon: number, lat: number): [number, number] {
  const [x, y] = equalEarth(lon, lat);
  return [R * (x + XMAX), R * (YMAX - y)];
}

/**
 * Where it's night now, as a path: under (or over) the terminator, run past the map's edges
 * so the blur that softens it into dusk never shows a seam. The sun's position is the textbook
 * approximation (declination by day of year, longitude by UTC time): plenty for a map.
 */
export function night(now: Date): string {
  const start = Date.UTC(now.getUTCFullYear(), 0, 0);
  const day = (now.getTime() - start) / 86_400_000;
  // Never exactly 0: at the equinox the terminator is a meridian and tan(0) divides by zero.
  const decl = -23.44 * Math.cos(((2 * Math.PI) / 365) * (day + 10)) || 0.01;
  const sunLon = -15 * (now.getUTCHours() + now.getUTCMinutes() / 60 - 12);
  const points: [number, number][] = [];
  for (let lon = -200; lon <= 200; lon += 2) {
    const lat = Math.atan(-Math.cos((lon - sunLon) * rad) / Math.tan(decl * rad)) / rad;
    points.push(project(lon, lat));
  }
  // Sun north of the equator: the night reaches the south pole, and the other way round.
  const far = decl > 0 ? 700 : -200;
  const [first, last] = [points[0], points[points.length - 1]];
  points.push([last[0] + 100, far], [first[0] - 100, far]);
  return 'M' + points.map(([x, y]) => `${x.toFixed(1)} ${y.toFixed(1)}`).join('L') + 'Z';
}
