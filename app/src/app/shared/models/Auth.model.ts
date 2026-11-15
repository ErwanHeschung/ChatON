export interface UserLogin {
  handle: string;
  nsec: string;
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
    | 'invalid'
    | 'keychain'
    | 'network'
    | 'signed_out'
    | 'internal';
  message: string;
}

export enum AuthStatus {
  LOADING,
  UNAUTHENTICATED,
  AUTHENTICATED,
}
