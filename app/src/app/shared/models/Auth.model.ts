export interface UserLogin {
  handle: string;
  nsec: string;
}

export interface LinkJoin {
  handle: string;
  code: string;
}

export interface UserRegister {
  instance: string;
  name: string;
}

/** The local account (core `session::Account`). */
export interface ConnectedUser {
  handle: string;
  relay: string;
  pubkey: string;
  npub: string;
}

/** Error returned by every Tauri command (`app/src-tauri/src/lib.rs`). */
export interface CmdError {
  code:
    | 'handle_taken'
    | 'not_found'
    | 'key_mismatch'
    | 'wrong_code'
    | 'code_expired'
    | 'link_refused'
    | 'invalid_invite'
    | 'refused'
    | 'too_large'
    | 'not_allowed'
    | 'invalid'
    | 'keychain'
    | 'network'
    | 'signed_out'
    | 'internal';
  message: string;
}

/** `link_request` event: a new device asks for the key, answer with `linkConfirm`. */
export interface LinkRequest {
  device_name: string;
}

/** `link_done` event: the link offer ended. */
export interface LinkDone {
  device_name: string | null;
  error: CmdError | null;
}

export enum AuthStatus {
  LOADING,
  UNAUTHENTICATED,
  AUTHENTICATED,
}
