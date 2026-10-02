/** Grupy Cognito odpowiadające rolom A–D z rozdziału 4 dokumentu projektu. */
export const USER_GROUPS = ['admin', 'staff', 'contributor', 'viewer'] as const;

export type UserGroup = (typeof USER_GROUPS)[number];

export const GROUP_LABELS: Record<UserGroup, string> = {
  admin: 'A · Admin',
  staff: 'B · Staff',
  contributor: 'C · Contributor',
  viewer: 'D · Viewer',
};

export function isUserGroup(value: unknown): value is UserGroup {
  return typeof value === 'string' && (USER_GROUPS as readonly string[]).includes(value);
}
