export interface UserShort {
  id: string;
  username: string;
  profilePicture: string;
  /** Remote picture URL, shown through `MediaService.picture`. */
  picture?: string | null;
}
