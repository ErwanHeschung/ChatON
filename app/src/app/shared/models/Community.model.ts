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
  /** Its category's id; null: above the categories. */
  category: string | null;
  topic: string | null;
  /** Seconds between two messages of a member, 0: off. */
  slowmode: number;
  /** Most people in a voice channel, 0: no limit. */
  user_limit: number;
  /** A member without roles can't see it (spec § 9, Salons privés). */
  private: boolean;
  overrides: ChannelOverride[];
}

/** What a channel allows and denies one target: a role id, `everyone` or a key. */
export interface ChannelOverride {
  target: string;
  allow: Permission[];
  deny: Permission[];
}

/** Groups channels; its id is stable, its name free (spec § 9, Salons et catégories). */
export interface Category {
  id: string;
  name: string;
}

/** What `edit_channel` changes; missing keeps it. */
export interface ChannelEdit {
  name?: string;
  topic?: string;
  slowmode?: number;
  user_limit?: number;
  /** Every override of the channel. */
  overrides?: ChannelOverride[];
}

export interface Community {
  id: string;
  relay: string;
  name: string;
  picture: string | null;
  channels: Channel[];
  /** In display order. */
  categories: Category[];
  /** My role ids, none for a plain member. */
  roles: string[];
  /** The owner's key, if the relay names one. */
  owner: string | null;
  /** My permissions in the community (spec § 9). */
  perms: Permission[];
  about: string | null;
  banner: string | null;
  /** Every role, highest first, `everyone` last. */
  all_roles: Role[];
  /** Roles strictly below mine: those I may edit, move, delete and give. */
  manageable_roles: string[];
  /** End of my timeout (unix seconds), 0: none. */
  timeout: number;
}

/** `["role", id, name, color, attr…]`: attrs are permissions, `hoist`, `mentionable` (spec § 9). */
export interface Role {
  id: string;
  name: string;
  /** `#rrggbb`, or empty. */
  color: string;
  attrs: string[];
}

export const EVERYONE = 'everyone';

/** What `update_community` changes; missing keeps it. */
export interface CommunityEdit {
  name?: string;
  about?: string;
  roles?: Role[];
  owner?: string;
  categories?: Category[];
  /** Every channel, in order, each in its category. */
  channels?: { id: string; category: string | null }[];
}

/**
 * Role colors to pick from: data, sent in the role tag, so plain hex. Readable on the frame
 * (4.5:1 and up), never the accent green (it means "me" and selection).
 */
export const ROLE_COLORS = [
  '#e5a50a',
  '#f6a86b',
  '#f48fb1',
  '#e879f9',
  '#c4a7ff',
  '#8ea2ff',
  '#60b8fa',
  '#3fd0e0',
  '#c9c5e8',
];

export interface Member {
  pubkey: string;
  roles: string[];
  owner: boolean;
  /** Below me in the hierarchy: I may act on them, given the permission. */
  manageable: boolean;
  /** Their highest colored role's color. */
  color: string | null;
  /** Their highest `hoist` role: the section they're listed under. */
  hoist: string | null;
  /** End of their timeout (unix seconds), 0: none. */
  timeout: number;
}

/** A running ban (spec § 9, Modération). */
export interface Ban {
  pubkey: string;
  by: string;
  reason: string | null;
  at: number;
}

/** One line of the audit log, newest first. */
export interface AuditEntry {
  id: string;
  at: number;
  actor: string;
  action:
    | 'roles'
    | 'timeout'
    | 'kick'
    | 'ban'
    | 'unban'
    | 'delete_message'
    | 'edit'
    | 'create_channel'
    | 'delete_channel'
    | 'leave';
  target: string | null;
  /** Where it happened, when it's a channel. */
  channel: string | null;
  reason: string | null;
  /** `roles`: those given. */
  roles: string[];
  /** `timeout`: its end, 0: lifted. */
  until: number;
}

/** What a moderator does to a member from the list. */
export type Moderation = 'kick' | 'ban' | 'timeout';

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
