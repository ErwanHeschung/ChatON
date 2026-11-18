// Presence (spec § 12), as core gives it.

/** What I choose, for all my devices. */
export type Status = 'online' | 'idle' | 'dnd' | 'invisible';

/** What I see of someone: no signal is offline. */
export type Presence = Exclude<Status, 'invisible'> | 'offline';

/** `presence` events from core's listener. */
export type PresenceUpdate =
  /** Everyone I see online; anyone else is offline. */
  | { type: 'presence'; online: Record<string, Presence> }
  /** Another device of mine changed my status. */
  | { type: 'status'; status: Status };

/** Labels for each status, mine (invisible) included. */
export const STATUS_LABELS: Record<Status | 'offline', string> = {
  online: $localize`:@@presence.online:Online`,
  idle: $localize`:@@presence.idle:Away`,
  dnd: $localize`:@@presence.dnd:Do not disturb`,
  invisible: $localize`:@@presence.invisible:Invisible`,
  offline: $localize`:@@presence.offline:Offline`,
};
