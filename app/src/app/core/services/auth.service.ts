import { inject, Injectable } from '@angular/core';
import { environment } from '@environments/environment';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';
import { ConnectedUser, UserLogin, UserRegister } from '@models/Auth.model';

@Injectable({
  providedIn: 'root',
})
export class AuthService {
  private readonly baseUrl = `${environment.apiUrl}/auth`;

  private readonly http = inject(HttpClient);

  public register(registerForm: UserRegister): Observable<ConnectedUser> {
    return this.http.post<ConnectedUser>(`${this.baseUrl}/register`, registerForm, {
      withCredentials: true,
    });
  }

  public login(loginForm: UserLogin): Observable<ConnectedUser> {
    return this.http.post<ConnectedUser>(`${this.baseUrl}/login`, loginForm, {
      withCredentials: true,
    });
  }

  public refreshToken(): Observable<ConnectedUser> {
    return this.http.post<ConnectedUser>(
      `${this.baseUrl}/refresh`,
      {},
      {
        withCredentials: true,
      },
    );
  }

  public logout(): Observable<void> {
    return this.http.post<void>(`${this.baseUrl}/logout`, {}, { withCredentials: true });
  }
}
