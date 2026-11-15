export interface Message {
  id: string;
  senderId: string;
  senderName: string;
  text: string;
  timestamp: Date;
  status: MessageStatus;
}

export enum MessageStatus {
  PENDING,
  SENT,
}
