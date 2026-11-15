// Shape returned by core (`media::Attachment`). The UI passes it back as is to open or save it.

export interface Attachment {
  url: string;
  mime: string;
  name: string;
  size: number;
  sha256: string;
  plain_sha256: string;
  key: string;
  nonce: string;
}

/** Same cap as core and the server (spec/protocol.md § 10): checked here to skip a useless IPC copy. */
export const MAX_FILE_SIZE = 25 * 1024 * 1024;
