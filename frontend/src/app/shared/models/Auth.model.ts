export interface UserLogin {
  username: string;
  password: string;
}

export interface UserRegister {
  username: string;
  email: string;
  password: string;
}

export interface ConnectedUser {
  id: string;
  username: string;
  email: string;
}
