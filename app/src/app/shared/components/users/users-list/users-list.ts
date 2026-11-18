import { ChangeDetectionStrategy, Component, computed, inject, LOCALE_ID } from '@angular/core';
import { CdkContextMenuTrigger, CdkMenu, CdkMenuItem, CdkMenuTrigger } from '@angular/cdk/menu';
import {
  LucideBan,
  LucideClock,
  LucideDynamicIcon,
  LucideEllipsis,
  LucideShield,
  LucideUserMinus,
  LucideUserPlus,
} from '@lucide/angular';
import { ModerateDialog } from '@components/servers/moderate-dialog/moderate-dialog';
import { UserCard } from '@components/users/user-card/user-card';
import { ProfileDirective } from '@directives/profile';
import { GroupDialog } from '@components/chats/group-dialog/group-dialog';
import {
  Member,
  Moderation,
  Permission,
  ROLE_ADMIN,
  ROLE_MODERATOR,
} from '@models/Community.model';
import { UserShort } from '@models/User.model';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** Members panel: the open community's members by role, or the open conversation's people. */
@Component({
  selector: 'app-users-list',
  imports: [
    CdkContextMenuTrigger,
    CdkMenu,
    CdkMenuItem,
    CdkMenuTrigger,
    UserCard,
    LucideDynamicIcon,
    ProfileDirective,
  ],
  templateUrl: './users-list.html',
  styleUrl: './users-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UsersList {
  private readonly account = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);
  protected readonly dm = inject(DmStore);
  protected readonly community = inject(CommunityStore);
  private readonly shell = inject(ShellService);
  protected readonly addPeopleIcon = LucideUserPlus;
  protected readonly icons = {
    more: LucideEllipsis,
    moderator: LucideShield,
    timeout: LucideClock,
    kick: LucideUserMinus,
    ban: LucideBan,
  };
  private readonly time = new Intl.DateTimeFormat(inject(LOCALE_ID), {
    dateStyle: 'short',
    timeStyle: 'short',
  });

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

  /** `perm` over a member below me (the relay checks too). */
  protected can(member: Member, perm: Permission): boolean {
    return this.community.can(perm) && member.manageable;
  }

  /** Something in the member's menu. */
  protected moderable(member: Member): boolean {
    return (
      this.canPromote(member) ||
      (['moderate_members', 'kick_members', 'ban_members'] as const).some((p) =>
        this.can(member, p),
      )
    );
  }

  protected timedOut(member: Member): boolean {
    return member.timeout * 1000 > Date.now();
  }

  protected timeoutLabel(member: Member): string {
    const until = this.time.format(member.timeout * 1000);
    return $localize`:@@moderate.timed-out:Timed out until ${until}:until:`;
  }

  protected moderate(action: Moderation, member: Member): void {
    this.shell.moderating.set({ action, pubkey: member.pubkey });
    this.modalService.open(ModerateDialog);
  }

  protected liftTimeout(member: Member): void {
    void this.community.moderate('timeout', member.pubkey, '', 0);
  }

  /** Adds or takes `moderator`, keeping their other roles. */
  public toggleModerator(member: Member): void {
    const others = member.roles.filter((r) => r !== ROLE_MODERATOR);
    void this.community.setRoles(
      member.pubkey,
      this.isModerator(member) ? others : [...others, ROLE_MODERATOR],
    );
  }
}
