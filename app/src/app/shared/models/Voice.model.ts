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
  /** Who shares a screen, me included. */
  sharing: string[];
  /** Whose screen I watch. */
  watching: string | null;
}

/** A screen or window I can share. */
export interface ScreenSource {
  id: number;
  title: string;
  /** A whole screen, else a window. */
  screen: boolean;
  /** What it shows now (a `data:` URL), if it could be captured. */
  thumbnail: string | null;
}

/** `voice` events from core's listener. */
export type VoiceUpdate =
  | { type: 'ring'; message: DmMessage; until: number }
  | { type: 'voice'; state: VoiceState | null }
  /** A moderator moves me: join `channel`. */
  | { type: 'voice_move'; community: string; channel: string };

/** What a moderator does to someone in a voice channel (spec § 11). */
export type VoiceAction = 'mute' | 'unmute' | 'deafen' | 'undeafen' | 'disconnect' | 'move';
