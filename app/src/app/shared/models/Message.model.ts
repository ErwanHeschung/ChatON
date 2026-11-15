import { Attachment } from '@models/Media.model';

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
  /** Shows a delete button: my own message, or I moderate the community. */
  deletable?: boolean;
}

export enum MessageStatus {
  PENDING,
  SENT,
}
