import { Attachment } from '@models/Media.model';

// Shapes returned by core (`community::*`). Keys and ids are hex.

export interface Channel {
  id: string;
  name: string;
}

export interface Community {
  id: string;
  relay: string;
  name: string;
  picture: string | null;
  channels: Channel[];
  /** My roles: `admin`, `moderator`, or none for a plain member. */
  roles: string[];
}

export interface Member {
  pubkey: string;
  roles: string[];
}

export interface ChannelMessage {
  id: string;
  channel: string;
  author: string;
  text: string;
  created_at: number;
  attachment: Attachment | null;
}

/** `community` events from core's listener. */
export type CommunityUpdate =
  | { type: 'channel_message'; message: ChannelMessage }
  | { type: 'deleted'; channel: string; ids: string[] }
  | { type: 'communities' };

export const ROLE_ADMIN = 'admin';
export const ROLE_MODERATOR = 'moderator';
