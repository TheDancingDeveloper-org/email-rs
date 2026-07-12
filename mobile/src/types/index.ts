export interface Account {
  id: string;
  name: string;
  email: string;
  providerType: string;
  authType: string;
  createdAt: string;
}

export interface Folder {
  id: string;
  accountId: string;
  name: string;
  fullPath: string;
  specialUse: string | null;
  unreadCount: number;
  totalCount: number;
  isExcluded: boolean;
}

export interface AccountSettings {
  id: string;
  name: string;
  email: string;
  syncDaysLimit: number | null;
}

export interface Message {
  id: string;
  accountId: string;
  folderId: string;
  subject: string | null;
  fromName: string | null;
  fromEmail: string | null;
  date: string | null;
  isRead: boolean;
  isFlagged: boolean;
  hasAttachments: boolean;
  preview: string | null;
  threadId: string | null;
}

export interface MessageBody {
  id: string;
  accountId: string;
  subject: string | null;
  fromName: string | null;
  fromEmail: string | null;
  date: string | null;
  isRead: boolean;
  isFlagged: boolean;
  hasAttachments: boolean;
  body: { htmlBody: string | null; textBody: string | null } | null;
  attachments: MessageAttachment[];
}

export interface MessageAttachment {
  id: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
}

export type Screen = 'inbox' | 'compose' | 'calendar' | 'profile';
