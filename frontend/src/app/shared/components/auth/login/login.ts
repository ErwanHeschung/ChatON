import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import {
  FormControl,
  FormGroup,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms';
import { Router } from '@angular/router';
import { ConnectedUser } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { CircleAlert, LucideAngularModule } from 'lucide-angular';
import { finalize } from 'rxjs';
import { ROUTES } from 'src/app/app.routes';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { ModalService } from '@services/modal.service';

interface LoginForm {
  username: FormControl<string>;
  password: FormControl<string>;
}

@Component({
  selector: 'app-login',
  imports: [ReactiveFormsModule, LucideAngularModule],
  templateUrl: './login.html',
  styleUrl: './login.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Login {
  private readonly fb = inject(NonNullableFormBuilder);
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);
  private readonly connectedUserStore = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);

  public isPending = signal<boolean>(false);
  public errorIcon = CircleAlert;

  public readonly loginForm: FormGroup<LoginForm> = this.fb.group({
    username: ['', [Validators.required, Validators.minLength(3)]],
    password: ['', [Validators.required, Validators.minLength(8)]],
  });

  public onSubmit(): void {
    this.isPending.set(true);
    if (this.loginForm.valid) {
      this.authService
        .login(this.loginForm.getRawValue())
        .pipe(finalize(() => this.isPending.set(false)))
        .subscribe({
          next: (user: ConnectedUser) => {
            this.connectedUserStore.setLoggedInUser(user);
            this.modalService.close();
            this.router.navigate([ROUTES.CHAT]);
          },
          error: (err) => {
            if (err.status === 401) {
              this.loginForm.setErrors({ unauthorized: true });
            } else {
              this.loginForm.setErrors({ unknown: true });
            }
          },
        });
    } else {
      this.loginForm.markAllAsTouched();
    }
  }
}
