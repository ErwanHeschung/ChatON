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
}

export interface Peer {
  /** Verified `pseudo@instance`, null if the key's profile doesn't resolve back to it. */
  handle: string | null;
  pubkey: string;
  npub: string;
  /** Profile picture URL, only if hosted on the peer's own instance. */
  picture: string | null;
}

/** A 1:1 (2 members) or a group (up to 10). */
export interface Conversation {
  room: string[];
  subject: string | null;
  last: DmMessage;
}

/** One string per conversation, the same for every member. */
export const roomKey = (room: string[]): string => room.join(',');

/** Always the full handle (never a bare name, CLAUDE.md), else a shortened npub. */
export function peerLabel(peer: Peer | undefined, pubkey: string): string {
  if (peer?.handle) {
    return `@${peer.handle}`;
  }
  const npub = peer?.npub ?? pubkey;
  return `${npub.slice(0, 12)}…${npub.slice(-6)}`;
}
