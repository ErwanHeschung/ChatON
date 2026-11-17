import { DmMessage } from '@models/Dm.model';

// Shapes returned by core (`voice::*`). Keys are hex.

/** Where I'm talking: a community's voice channel, or a DM call. */
export type VoiceTarget =
  | { type: 'channel'; community: string; channel: string }
  | { type: 'call'; room: string[]; id: string };

/** The voice channel or call I'm in. */
export interface VoiceState {
  target: VoiceTarget;
  connection: 'connected' | 'reconnecting';
  muted: boolean;
  /** Hears nobody; the microphone is off too. */
  deafened: boolean;
  /** False: listening only (no microphone, or not allowed). */
  microphone: boolean;
  /** End-to-end encrypted (DM calls). */
  encrypted: boolean;
  /** Me included. */
  participants: string[];
  speaking: string[];
}

/** `voice` events from core's listener. */
export type VoiceUpdate =
  { type: 'ring'; message: DmMessage; until: number } | { type: 'voice'; state: VoiceState | null };
