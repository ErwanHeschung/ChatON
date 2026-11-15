import { computed, Injectable, signal, Type } from '@angular/core';

@Injectable({
  providedIn: 'root',
})
export class ModalService {
  private readonly component = signal<Type<unknown> | null>(null);

  public readonly activeComponent = this.component.asReadonly();

  public readonly isOpen = computed(() => this.component() !== null);

  public open<T>(component: Type<T>): void {
    this.component.set(component);
  }

  public close(): void {
    this.component.set(null);
  }
}
