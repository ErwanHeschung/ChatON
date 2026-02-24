export interface Message {
  id: string;
  senderId: string;
  text: string;
  timestamp: Date;
  status: MessageStatus;
}

export enum MessageStatus {
  PENDING,
  SENT,
}
