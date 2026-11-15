export interface Message {
  id: string;
  senderId: string;
  senderName: string;
  text: string;
  timestamp: Date;
  status: MessageStatus;
  /** Shows a delete button: my own message, or I moderate the community. */
  deletable?: boolean;
}

export enum MessageStatus {
  PENDING,
  SENT,
}
