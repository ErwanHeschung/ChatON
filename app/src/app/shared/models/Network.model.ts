/** An instance I know (core `network::Instance`). */
export interface Instance {
  domain: string;
  mine: boolean;
  /** My contacts there (keys). */
  people: string[];
  communities: number;
  blocked: boolean;
  reachable: boolean;
  latency_ms: number | null;
  /** NIP-11 name. */
  name: string | null;
  /** NIP-11 `relay_countries`, as its admin says (ISO 3166 alpha-2). */
  countries: string[];
}
