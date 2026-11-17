import { Attachment } from '@models/Media.model';

// Shapes returned by core (`community::*`). Keys and ids are hex.

export interface Channel {
  id: string;
  name: string;
  /** A voice channel (spec § 11). */
  voice: boolean;
  /** Who is in it (voice channels), by the relay. */
  participants: string[];
  /** My permissions in this channel (spec § 9). */
  perms: Permission[];
}

export interface Community {
  id: string;
  relay: string;
  name: string;
  picture: string | null;
  channels: Channel[];
  /** My role ids, none for a plain member. */
  roles: string[];
  /** The owner's key, if the relay names one. */
  owner: string | null;
  /** My permissions in the community (spec § 9). */
  perms: Permission[];
}

export interface Member {
  pubkey: string;
  roles: string[];
  owner: boolean;
  /** Below me in the hierarchy: I may act on them, given the permission. */
  manageable: boolean;
}

/** Permission names (spec § 9); core computes who has which. */
export type Permission =
  | 'administrator'
  | 'manage_community'
  | 'manage_channels'
  | 'manage_roles'
  | 'manage_invites'
  | 'create_invite'
  | 'kick_members'
  | 'ban_members'
  | 'moderate_members'
  | 'view_audit_log'
  | 'manage_messages'
  | 'mention_everyone'
  | 'view_channel'
  | 'send_messages'
  | 'attach_files'
  | 'add_reactions'
  | 'connect'
  | 'speak'
  | 'stream'
  | 'mute_members'
  | 'move_members';

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
  | { type: 'communities' }
  | { type: 'blocks' };

export const ROLE_ADMIN = 'admin';
export const ROLE_MODERATOR = 'moderator';
