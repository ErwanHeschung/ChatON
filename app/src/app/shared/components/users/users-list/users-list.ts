import { ChangeDetectionStrategy, Component, computed, effect, inject } from '@angular/core';
import { UserCard } from '@components/users/user-card/user-card';
import { GroupDialog } from '@components/chats/group-dialog/group-dialog';
import { AccountSettings } from '@components/account-settings/account-settings';
import { Member, ROLE_ADMIN, ROLE_MODERATOR } from '@models/Community.model';
import { UserShort } from '@models/User.model';
import { ModalService } from '@services/modal.service';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** Right sidebar: me, then the members of the open community. */
@Component({
  selector: 'app-users-list',
  imports: [UserCard],
  templateUrl: './users-list.html',
  styleUrl: './users-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UsersList {
  private readonly account = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);
  protected readonly dm = inject(DmStore);
  protected readonly community = inject(CommunityStore);

  public readonly connectedUser = computed<UserShort>(() => ({
    id: this.account.user()?.npub ?? '',
    username: this.account.user() ? `@${this.account.user()?.handle}` : '',
    profilePicture: '/chatON-logo.svg',
    picture: this.dm.peers()[this.account.user()?.pubkey ?? '']?.picture,
  }));

  /** Admins, then moderators, then the rest. Only in the server view. */
  public readonly members = computed(() => {
    if (this.community.channel() === null) {
      return [];
    }
    const rank = (m: Member) =>
      m.roles.includes(ROLE_ADMIN) ? 0 : m.roles.includes(ROLE_MODERATOR) ? 1 : 2;
    return [...this.community.members()].sort((a, b) => rank(a) - rank(b));
  });

  constructor() {
    // My own profile, for my picture.
    effect(() => {
      const me = this.account.user()?.pubkey;
      if (me) void this.dm.resolve(me);
    });
  }

  /** The open group's members (DM view only). */
  public readonly groupMembers = computed(() =>
    this.community.channel() === null && this.dm.isGroup() ? this.dm.room() : [],
  );

  public cardOf(pubkey: string): UserShort {
    return {
      id: pubkey,
      username: this.dm.label(pubkey),
      profilePicture: '/chatON-logo.svg',
      picture: this.dm.peers()[pubkey]?.picture,
    };
  }

  public rename(input: HTMLInputElement): void {
    void this.dm.rename(input.value);
  }

  /** A new group, starting from these members: membership never changes (spec § 5). */
  public addPeople(): void {
    this.dm.setGroupDraft(this.dm.others(this.dm.selected() ?? ''));
    this.modalService.open(GroupDialog);
  }

  public openAccount(): void {
    this.modalService.open(AccountSettings);
  }

  public card(member: Member): UserShort {
    return this.cardOf(member.pubkey);
  }

  public isMe(member: Member): boolean {
    return member.pubkey === this.account.user()?.pubkey;
  }

  public isAdmin(member: Member): boolean {
    return member.roles.includes(ROLE_ADMIN);
  }

  public isModerator(member: Member): boolean {
    return member.roles.includes(ROLE_MODERATOR);
  }

  /** Admins remove anyone; moderators everyone but admins (the relay checks too). */
  public canRemove(member: Member): boolean {
    return (
      !this.isMe(member) &&
      (this.community.isAdmin() || (!!this.community.canModerate() && !this.isAdmin(member)))
    );
  }

  public toggleModerator(member: Member): void {
    void this.community.setRoles(member.pubkey, this.isModerator(member) ? [] : [ROLE_MODERATOR]);
  }

  public remove(member: Member): void {
    void this.community.removeMember(member.pubkey);
  }
}
