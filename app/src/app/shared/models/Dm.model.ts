// Shapes returned by core (`dm::Message`, `session::{Peer, Conversation}`). Keys are hex.

export interface DmMessage {
  id: string;
  sender: string;
  receiver: string;
  text: string;
  created_at: number;
}

export interface Peer {
  /** Verified `pseudo@instance`, null if the key's profile doesn't resolve back to it. */
  handle: string | null;
  pubkey: string;
  npub: string;
}

export interface Conversation {
  peer: string;
  last: DmMessage;
}

/** Always the full handle (never a bare name, CLAUDE.md), else a shortened npub. */
export function peerLabel(peer: Peer | undefined, pubkey: string): string {
  if (peer?.handle) {
    return `@${peer.handle}`;
  }
  const npub = peer?.npub ?? pubkey;
  return `${npub.slice(0, 12)}…${npub.slice(-6)}`;
}
