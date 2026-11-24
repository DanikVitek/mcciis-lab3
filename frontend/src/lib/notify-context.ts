import { createContext } from 'svelte';

export type NoticeKind = 'ok' | 'info' | 'error';
export type NotifyFn = (kind: NoticeKind, text: string) => void;

export const [getNotify, setNotify] = createContext<NotifyFn>();
