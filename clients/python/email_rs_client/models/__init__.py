"""Contains all the data models used in inputs/outputs"""

from .account import Account
from .account_candidate import AccountCandidate
from .account_health import AccountHealth
from .account_settings import AccountSettings
from .action_input import ActionInput
from .add_link_body import AddLinkBody
from .add_link_response import AddLinkResponse
from .attendee import Attendee
from .auth_client_kind import AuthClientKind
from .auth_credentials import AuthCredentials
from .auth_identity import AuthIdentity
from .auth_session import AuthSession
from .authentication_result_status import AuthenticationResultStatus
from .authorize_response import AuthorizeResponse
from .bulk_action import BulkAction
from .bulk_request import BulkRequest
from .bulk_response import BulkResponse
from .bulk_result import BulkResult
from .calendar_event import CalendarEvent
from .calendar_search_result import CalendarSearchResult
from .calendar_suggestion import CalendarSuggestion
from .cancel_outbox_response import CancelOutboxResponse
from .capabilities_response import CapabilitiesResponse
from .condition_input import ConditionInput
from .contact import Contact
from .create_account import CreateAccount
from .create_folder import CreateFolder
from .create_identity import CreateIdentity
from .create_integration_token import CreateIntegrationToken
from .create_label_body import CreateLabelBody
from .create_rule_body import CreateRuleBody
from .create_webhook import CreateWebhook
from .created_integration_token import CreatedIntegrationToken
from .cursor_messages_response import CursorMessagesResponse
from .discovery_request import DiscoveryRequest
from .discovery_response import DiscoveryResponse
from .draft_request import DraftRequest
from .draft_response import DraftResponse
from .error_response import ErrorResponse
from .event_link import EventLink
from .folder import Folder
from .full_message import FullMessage
from .health_details import HealthDetails
from .identity import Identity
from .index_admin_response import IndexAdminResponse
from .integration_event import IntegrationEvent
from .integration_token import IntegrationToken
from .invitation import Invitation
from .invitation_attendee import InvitationAttendee
from .label import Label
from .list_unsubscribe import ListUnsubscribe
from .mail_endpoint import MailEndpoint
from .message import Message
from .message_attachment import MessageAttachment
from .message_authentication_results import MessageAuthenticationResults
from .message_body import MessageBody
from .message_suggestion import MessageSuggestion
from .move_message_request import MoveMessageRequest
from .move_message_response import MoveMessageResponse
from .outbox_item import OutboxItem
from .outgoing_attachment import OutgoingAttachment
from .patch_folder import PatchFolder
from .patch_message_request import PatchMessageRequest
from .rename_folder import RenameFolder
from .rsvp_request import RsvpRequest
from .rule_action_dto import RuleActionDto
from .rule_condition_dto import RuleConditionDto
from .rule_dto import RuleDto
from .search_response import SearchResponse
from .send_request import SendRequest
from .send_response import SendResponse
from .snooze_request import SnoozeRequest
from .status_response import StatusResponse
from .suggest_response import SuggestResponse
from .unsubscribe_response import UnsubscribeResponse
from .update_account import UpdateAccount
from .update_identity import UpdateIdentity
from .update_label_body import UpdateLabelBody
from .update_rule_body import UpdateRuleBody
from .update_webhook import UpdateWebhook
from .version_response import VersionResponse
from .webhook import Webhook
from .webhook_delivery import WebhookDelivery

__all__ = (
    "Account",
    "AccountCandidate",
    "AccountHealth",
    "AccountSettings",
    "ActionInput",
    "AddLinkBody",
    "AddLinkResponse",
    "Attendee",
    "AuthClientKind",
    "AuthCredentials",
    "AuthenticationResultStatus",
    "AuthIdentity",
    "AuthorizeResponse",
    "AuthSession",
    "BulkAction",
    "BulkRequest",
    "BulkResponse",
    "BulkResult",
    "CalendarEvent",
    "CalendarSearchResult",
    "CalendarSuggestion",
    "CancelOutboxResponse",
    "CapabilitiesResponse",
    "ConditionInput",
    "Contact",
    "CreateAccount",
    "CreatedIntegrationToken",
    "CreateFolder",
    "CreateIdentity",
    "CreateIntegrationToken",
    "CreateLabelBody",
    "CreateRuleBody",
    "CreateWebhook",
    "CursorMessagesResponse",
    "DiscoveryRequest",
    "DiscoveryResponse",
    "DraftRequest",
    "DraftResponse",
    "ErrorResponse",
    "EventLink",
    "Folder",
    "FullMessage",
    "HealthDetails",
    "Identity",
    "IndexAdminResponse",
    "IntegrationEvent",
    "IntegrationToken",
    "Invitation",
    "InvitationAttendee",
    "Label",
    "ListUnsubscribe",
    "MailEndpoint",
    "Message",
    "MessageAttachment",
    "MessageAuthenticationResults",
    "MessageBody",
    "MessageSuggestion",
    "MoveMessageRequest",
    "MoveMessageResponse",
    "OutboxItem",
    "OutgoingAttachment",
    "PatchFolder",
    "PatchMessageRequest",
    "RenameFolder",
    "RsvpRequest",
    "RuleActionDto",
    "RuleConditionDto",
    "RuleDto",
    "SearchResponse",
    "SendRequest",
    "SendResponse",
    "SnoozeRequest",
    "StatusResponse",
    "SuggestResponse",
    "UnsubscribeResponse",
    "UpdateAccount",
    "UpdateIdentity",
    "UpdateLabelBody",
    "UpdateRuleBody",
    "UpdateWebhook",
    "VersionResponse",
    "Webhook",
    "WebhookDelivery",
)
