import { Attachment } from '@models/Media.model';
import { Reaction } from '@models/Community.model';

export interface Message {
  id: string;
  senderId: string;
  senderName: string;
  /** Remote picture URL, shown through `MediaService.picture`. */
  senderPicture?: string | null;
  text: string;
  timestamp: Date;
  status: MessageStatus;
  attachment?: Attachment | null;
  /** A group (re)naming: shown as a note, not a bubble. */
  subject?: string | null;
  /** A call started or ended: a note. `joinable`: started, not ended, and I'm not in it. */
  call?: { id: string; ended: boolean; joinable: boolean } | null;
  /** Sent by me: my name shows in green. */
  mine?: boolean;
  /** Shows a delete button: my own message, or I moderate the community. */
  deletable?: boolean;
  /** Answers a message: its author and text (null: it's gone). */
  reply?: { name: string; text: string | null } | null;
  reactions?: Reaction[];
  pinned?: boolean;
  /** Text to highlight as mentions (`@lea@miaou.fr`, `@Gardien`, `@everyone`). */
  mentions?: string[];
  /** It mentions me: the row stands out. */
  mentionsMe?: boolean;
  /** What I may do with it (channels): reply, react, pin. */
  can?: { reply: boolean; react: boolean; pin: boolean };
}

/** Someone or something `@` can mention: what goes in the tags (`id`) and in the text. */
export interface MentionCandidate {
  /** A key (hex), a role id, or `everyone`. */
  id: string;
  kind: 'person' | 'role' | 'everyone';
  /** Shown in the list: a display name, a role's name. */
  name: string;
  /** Inserted in the message: `@lea@miaou.fr`, `@Gardien`, `@everyone`. */
  text: string;
  picture?: string | null;
  /** A role's color. */
  color?: string;
}

export enum MessageStatus {
  PENDING,
  SENT,
}
