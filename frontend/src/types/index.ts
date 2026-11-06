export interface Account {
  id: string;
  name: string;
  email: string;
  providerType: 'gmail' | 'microsoft365' | 'imap' | 'exchange';
  color?: string;
  /** The provider refused the OAuth grant; sync is paused until reconnect. */
  needsReauth?: boolean;
  reauthReason?: string;
}

export interface AccountSettings {
  id: string;
  name: string;
  email: string;
  providerType: string;
  syncDaysLimit: number | null;
  signature: string | null;
}

export interface Folder {
  id: string;
  accountId: string;
  name: string;
  fullPath: string;
  specialUse: 'inbox' | 'sent' | 'drafts' | 'archive' | 'trash' | 'spam' | null;
  unreadCount: number;
  totalCount: number;
  isExcluded: boolean;
}

export interface Message {
  id: string;
  folderId: string;
  accountId: string;
  subject: string | null;
  fromName: string | null;
  fromEmail: string | null;
  date: string | null;
  isRead: boolean;
  isFlagged: boolean;
  hasAttachments: boolean;
  preview: string | null;
  threadId: string | null;
  snoozedUntil?: string | null;
}

export interface MessageBody {
  messageId: string;
  htmlBody: string | null;
  textBody: string | null;
}

export interface Attendee {
  email: string;
  name: string | null;
  responseStatus: string | null;
}

export interface CalendarEvent {
  id: string;
  calendarId: string;
  title: string;
  description: string | null;
  startAt: string;
  endAt: string;
  location: string | null;
  isAllDay: boolean;
  meetLink: string | null;
  attendees: Attendee[];
}

// ── Condition model — shared by advanced search and rules engine ─────────────

export type ConditionField =
  | 'from' | 'to' | 'cc' | 'subject' | 'body' | 'filename'
  | 'has_attachment' | 'is_read' | 'is_flagged'
  | 'date_after' | 'date_before';

export type ConditionOperator = 'contains' | 'not_contains' | 'equals' | 'starts_with';

export interface Condition {
  id: string;
  field: ConditionField;
  operator: ConditionOperator;
  // text fields: string value; boolean fields: 'true'|'false'; date fields: 'YYYY-MM-DD'
  value: string;
}

export interface ConditionGroup {
  match: 'all' | 'any';
  conditions: Condition[];
}

export interface Suggestion {
  id: string;
  folderId: string;
  subject: string | null;
  fromName: string | null;
  fromEmail: string | null;
  date: string | null;
}

export interface CalendarSuggestion {
  id: string;
  title: string;
  startAt: string;
}

export interface SuggestResponse {
  messages: Suggestion[];
  events: CalendarSuggestion[];
}

export interface CalendarSearchResult {
  id: string;
  title: string;
  description: string | null;
  startAt: string;
  endAt: string;
  location: string | null;
  isAllDay: boolean;
}

export interface SearchResponse {
  messages: Message[];
  events: CalendarSearchResult[];
}

export interface Label {
  id: string;
  accountId: string;
  name: string;
  color: string;
}

export interface EventLink {
  id: string;
  eventId: string;
  messageId: string;
  folderId: string;
  subject: string | null;
  fromName: string | null;
  fromEmail: string | null;
  date: string | null;
  linkedAt: string;
}
