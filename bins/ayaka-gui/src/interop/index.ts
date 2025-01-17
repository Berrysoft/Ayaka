import { convertFileSrc, invoke } from "@tauri-apps/api/tauri"
import { Locale } from 'vue-i18n'

export function conv_src(path?: string): string | undefined {
    if (path) {
        return decodeURIComponent(convertFileSrc(path))
    }
    return undefined
}

export interface OpenGameStatus {
    t: keyof typeof OpenGameStatusType,
    data?: object,
}

export enum OpenGameStatusType {
    LoadProfile,
    CreateRuntime,
    LoadPlugin,
    LoadSettings,
    LoadGlobalRecords,
    LoadRecords,
    Loaded,
}

export interface Settings {
    lang: Locale,
}

export interface RawContext {
    cur_para: string,
    cur_act: number,
    history: Action[],
    bg?: string,
    bgm?: string,
}

export interface GameInfo {
    title: string,
    author: string,
    props: {
        bg?: string,
    },
}

export interface Action {
    line: ActionLine[],
    ch_key?: string,
    character?: string,
    para_title?: string,
    switches: Switch[],
    props: {
        bg?: string,
        bgm?: string,
        efm?: string,
        voice?: string,
        video?: string,
        ch_models_count?: string,
    },
}

export interface ActionLine {
    type: keyof typeof ActionLineType,
    data: string
}

export enum ActionLineType {
    Chars,
    Block,
}

export interface Switch {
    text: string,
    enabled: boolean,
}

export function ayaka_version(): Promise<string> {
    return invoke("ayaka_version")
}

export function open_game(): Promise<void> {
    return invoke("open_game")
}

export function get_settings(): Promise<Settings | undefined> {
    return invoke("get_settings")
}

export function set_settings(settings: Settings): Promise<void> {
    return invoke("set_settings", { settings: settings })
}

export function get_records(): Promise<RawContext[]> {
    return invoke("get_records")
}

export function save_record_to(index: number): Promise<void> {
    return invoke("save_record_to", { index: index })
}

export async function set_locale(loc: Locale): Promise<void> {
    let settings = await get_settings() ?? { lang: "" };
    settings.lang = loc
    await set_settings(settings)
}

export function save_all(): Promise<void> {
    return invoke("save_all")
}

export function choose_locale(locales: Locale[]): Promise<Locale | undefined> {
    return invoke("choose_locale", { locales: locales })
}

export function locale_native_name(loc: Locale): string {
    return new Intl.DisplayNames(loc, { type: "language" }).of(loc) ?? ""
}

export async function info(): Promise<GameInfo> {
    let res = await invoke<GameInfo | undefined>("info");
    return res ?? { title: "", author: "", props: {} }
}

export function start_new(locale: Locale): Promise<void> {
    return invoke("start_new", { locale: locale })
}

export function start_record(locale: Locale, index: number): Promise<void> {
    return invoke("start_record", { locale: locale, index: index })
}

export function next_run(): Promise<boolean> {
    return invoke("next_run")
}

export function next_back_run(): Promise<boolean> {
    return invoke("next_back_run")
}

export function current_run(): Promise<Action | undefined> {
    return invoke("current_run")
}

export async function current_visited(): Promise<boolean> {
    return invoke("current_visited")
}

export function switch_(i: number): Promise<void> {
    return invoke("switch", { i: i })
}

export function history(): Promise<Action[]> {
    return invoke("history")
}

export function merge_lines(lines: ActionLine[]): string {
    let res = ""
    lines.forEach(s => {
        res += s.data
    })
    return res
}
