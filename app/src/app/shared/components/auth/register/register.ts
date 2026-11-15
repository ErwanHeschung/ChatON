import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import {
  FormControl,
  FormGroup,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms';
import { Router } from '@angular/router';
import { CmdError, ConnectedUser } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { LucideCircleExclamationPoint, LucideDynamicIcon } from '@lucide/angular';
import { finalize, switchMap, tap } from 'rxjs';
import { ROUTES } from '@app/app.routes';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { ModalService } from '@services/modal.service';
import { environment } from '@environments/environment';

interface RegisterForm {
  instance: FormControl<string>;
  name: FormControl<string>;
}

/** Same rule as the directory (spec/federation.md § 1), case-insensitive. */
const NAME_PATTERN = /^[a-zA-Z0-9._-]{3,32}$/;
const INSTANCE_PATTERN = /^https?:\/\/[^\s/]+\/?$/;

@Component({
  selector: 'app-register',
  imports: [ReactiveFormsModule, LucideDynamicIcon],
  templateUrl: './register.html',
  styleUrl: '../login/login.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Register {
  public isPending = signal<boolean>(false);
  /** Set once registered: the secret key to back up before going on. */
  public readonly nsec = signal<string | null>(null);
  public readonly copied = signal<boolean>(false);
  public errorIcon = LucideCircleExclamationPoint;

  private readonly router = inject(Router);
  private readonly fb = inject(NonNullableFormBuilder);
  private readonly authService = inject(AuthService);
  private readonly connectedUserStore = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);

  public readonly registerForm: FormGroup<RegisterForm> = this.fb.group({
    instance: [
      environment.defaultInstance,
      [Validators.required, Validators.pattern(INSTANCE_PATTERN)],
    ],
    name: ['', [Validators.required, Validators.pattern(NAME_PATTERN)]],
  });

  private readonly value = toSignal(this.registerForm.valueChanges, {
    initialValue: this.registerForm.getRawValue(),
  });

  /** The handle the user is about to get, shown in full (`pseudo@instance`). */
  public readonly preview = computed(() => {
    const { instance, name } = this.value();
    const host = instance?.replace(/^https?:\/\//, '').replace(/\/$/, '');
    return name && host ? `@${name.toLowerCase()}@${host}` : '';
  });

  public onSubmit(): void {
    if (this.registerForm.invalid) {
      this.registerForm.markAllAsTouched();
      return;
    }
    this.isPending.set(true);
    this.authService
      .register(this.registerForm.getRawValue())
      .pipe(
        tap((user: ConnectedUser) => this.connectedUserStore.setLoggedInUser(user)),
        switchMap(() => this.authService.exportKey()),
        finalize(() => this.isPending.set(false)),
      )
      .subscribe({
        next: (nsec: string) => this.nsec.set(nsec),
        error: (err: CmdError) => {
          const { name, instance } = this.registerForm.controls;
          switch (err.code) {
            case 'handle_taken':
              name.setErrors({ conflict: true });
              break;
            case 'invalid':
              name.setErrors({ pattern: true });
              break;
            case 'network':
              instance.setErrors({ unreachable: true });
              break;
            case 'keychain':
              this.registerForm.setErrors({ keychain: true });
              break;
            default:
              this.registerForm.setErrors({ unknown: true });
          }
        },
      });
  }

  public copy(): void {
    navigator.clipboard.writeText(this.nsec() ?? '').then(() => this.copied.set(true));
  }

  /** No vault: losing every device without this key loses the account. */
  public saved(): void {
    this.modalService.close();
    this.router.navigate([ROUTES.CHAT, ROUTES.DMS]);
  }
}
