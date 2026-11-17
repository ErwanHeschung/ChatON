import { Attachment } from '@models/Media.model';

// Shapes returned by core (`dm::Message`, `session::{Peer, Conversation}`). Keys are hex.

export interface DmMessage {
  id: string;
  sender: string;
  /** Every member, sender included, sorted by core: the conversation (spec § 5). */
  room: string[];
  text: string;
  created_at: number;
  /** Group name set by this message (empty text then). */
  subject: string | null;
  attachment: Attachment | null;
  /** A call invite or its end (empty text then). */
  call: { id: string; ended: boolean } | null;
}

export interface Peer {
  /** Verified `pseudo@instance`, null if the key's profile doesn't resolve back to it. */
  handle: string | null;
  pubkey: string;
  npub: string;
  /** Profile picture URL, only if hosted on the peer's own instance. */
  picture: string | null;
  /** Free name, not unique: always shown with the handle. */
  display_name: string | null;
}

/** A 1:1 (2 members) or a group (up to 10). */
export interface Conversation {
  room: string[];
  subject: string | null;
  last: DmMessage;
  /** A message request: I never wrote here. Doesn't notify, listed apart. */
  request: boolean;
}

/** My blocks (core `block::Blocks`), private to me. */
export interface Blocks {
  keys: string[];
  /** Domains: `b.fr`, `localhost:8098`. */
  instances: string[];
}

/** What my instance refuses (core `block::Rules`). */
export interface Privacy {
  blocks: Blocks;
  rules: { mode: string; instances: string[] };
}

/** "lea@chaton.fr" → "chaton.fr". */
export const instanceOf = (handle: string | null | undefined): string | null =>
  handle?.split('@')[1]?.toLowerCase() ?? null;

/** One string per conversation, the same for every member. */
export const roomKey = (room: string[]): string => room.join(',');

/** "@lea@chaton.fr" → ["@lea", "@chaton.fr"], so the UI shows the instance apart; else [label, ""]. */
export function splitHandle(label: string): [string, string] {
  const at = label.lastIndexOf('@');
  return label.startsWith('@') && at > 0 ? [label.slice(0, at), label.slice(at)] : [label, ''];
}

/** Always the full handle (never a bare name, CLAUDE.md), else a shortened npub. */
export function peerLabel(peer: Peer | undefined, pubkey: string): string {
  if (peer?.handle) {
    return `@${peer.handle}`;
  }
  const npub = peer?.npub ?? pubkey;
  return `${npub.slice(0, 12)}…${npub.slice(-6)}`;
}
