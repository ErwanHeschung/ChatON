import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { LucideDynamicIcon, LucideUserPlus } from '@lucide/angular';
import { UserCard } from '@components/users/user-card/user-card';
import { ProfileDirective } from '@directives/profile';
import { GroupDialog } from '@components/chats/group-dialog/group-dialog';
import { Member, ROLE_ADMIN, ROLE_MODERATOR } from '@models/Community.model';
import { UserShort } from '@models/User.model';
import { ModalService } from '@services/modal.service';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** Members panel: the open community's members by role, or the open conversation's people. */
@Component({
  selector: 'app-users-list',
  imports: [UserCard, LucideDynamicIcon, ProfileDirective],
  templateUrl: './users-list.html',
  styleUrl: './users-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UsersList {
  private readonly account = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);
  protected readonly dm = inject(DmStore);
  protected readonly community = inject(CommunityStore);
  protected readonly addPeopleIcon = LucideUserPlus;

  /** One section per role shown apart (`hoist`, in role order), then the rest. Server view only. */
  public readonly sections = computed(() => {
    const members = this.community.channel() === null ? [] : this.community.members();
    const hoisted = (this.community.current()?.all_roles ?? []).filter((r) =>
      r.attrs.includes('hoist'),
    );
    return [
      ...hoisted.map((r) => ({
        id: r.id,
        name: r.name as string | null,
        members: members.filter((m) => m.hoist === r.id),
      })),
      { id: '', name: null, members: members.filter((m) => !m.hoist) },
    ].filter((s) => s.members.length);
  });

  /** The open conversation's people (DM view only). */
  public readonly roomMembers = computed(() =>
    this.community.channel() === null ? this.dm.room() : [],
  );

  protected readonly me = computed(() => this.account.user()?.pubkey);

  public cardOf(pubkey: string): UserShort {
    return {
      id: pubkey,
      username: this.dm.label(pubkey),
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

  /** Names or unnames moderators below me, never touching an admin (the relay checks too). */
  public canPromote(member: Member): boolean {
    return this.community.can('manage_roles') && member.manageable && !this.isAdmin(member);
  }

  /** Removes members below me (the relay checks too). */
  public canRemove(member: Member): boolean {
    return this.community.can('kick_members') && member.manageable;
  }

  /** Adds or takes `moderator`, keeping their other roles. */
  public toggleModerator(member: Member): void {
    const others = member.roles.filter((r) => r !== ROLE_MODERATOR);
    void this.community.setRoles(
      member.pubkey,
      this.isModerator(member) ? others : [...others, ROLE_MODERATOR],
    );
  }

  public remove(member: Member): void {
    void this.community.removeMember(member.pubkey);
  }
}
