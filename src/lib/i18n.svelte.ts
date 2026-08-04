export type Language = 'system' | 'en' | 'zh' | 'zh_tw' | 'ja' | 'ko' | 'es' | 'fr' | 'de';
type ResolvedLanguage = Exclude<Language, 'system'>;
type Dictionary = Record<string, string>;

import { loadSettings, saveSetting } from '$lib/settings';

const dictionaries: Record<ResolvedLanguage, Dictionary> = {
  en: {
    // Shared / Navigation
    'nav.back': 'Back to Analytics',
    'nav.general': 'General',
    'nav.appearance': 'Appearance',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      'HarnessLens analyzes the effectiveness of AI coding harness usage, providing clear data to guide harness improvements.',

    // Analytics Dashboard
    'dashboard.eyebrow': 'Local Codex analytics',
    'dashboard.title': 'Coding Agent analytics',
    'dashboard.subtitle': 'Analyze local sessions, turns, token usage, and skill calls.',
    'dashboard.refresh': 'Refresh analytics',
    'dashboard.loading': 'Loading local analytics…',
    'dashboard.summary.label': 'Analytics summary',
    'dashboard.summary.sessions': 'Sessions',
    'dashboard.summary.tokens': 'Session tokens',
    'dashboard.summary.skills': 'Skill calls',
    'dashboard.turns.label': 'Turn outcomes',
    'dashboard.turns.succeeded': '{count} succeeded',
    'dashboard.turns.failed': '{count} failed',
    'dashboard.turns.cancelled': '{count} cancelled',
    'dashboard.turns.active': '{count} active',
    'dashboard.turns.unknown': '{count} unknown',
    'dashboard.views.label': 'Analytics views',
    'dashboard.views.sessions': 'Sessions',
    'dashboard.views.skills': 'Skills',
    'dashboard.filter.label': 'Session scope',
    'dashboard.filter.all': 'All',
    'dashboard.filter.current': 'Current',
    'dashboard.filter.archived': 'Archived',
    'dashboard.search.label': 'Search analytics',
    'dashboard.search.sessions': 'Search sessions or projects',
    'dashboard.search.skills': 'Search skills',
    'dashboard.pagination.label': 'Analytics pagination',
    'dashboard.pagination.previous': 'Previous page',
    'dashboard.pagination.next': 'Next page',
    'dashboard.pagination.page': 'Page {page} of {total}',
    'dashboard.session.name': 'Session',
    'dashboard.session.updated': 'Last activity',
    'dashboard.session.duration': 'Active time',
    'dashboard.session.tokens': 'Tokens',
    'dashboard.session.turns': 'Turns',
    'dashboard.session.skills': 'Skills',
    'dashboard.session.status': 'Latest outcome',
    'dashboard.session.untitled': 'Untitled session',
    'dashboard.session.unknown_project': 'Unknown project',
    'dashboard.session.wall_duration': 'Elapsed from creation to last activity: {duration}',
    'dashboard.skill.name': 'Skill',
    'dashboard.skill.invocations': 'Runs',
    'dashboard.skill.outcomes': 'Success / fail / cancel / other',
    'dashboard.skill.outcome_values':
      '{succeeded} succeeded, {failed} failed, {cancelled} cancelled, {unknown} active or unknown',
    'dashboard.skill.success_rate': 'Success',
    'dashboard.skill.average_duration': 'Avg time',
    'dashboard.skill.max_duration': 'Max time',
    'dashboard.skill.average_tokens': 'Avg tokens',
    'dashboard.skill.max_tokens': 'Max tokens',
    'dashboard.empty.sessions': 'No Codex sessions have been imported yet.',
    'dashboard.empty.skills': 'No Skill calls were detected.',
    'dashboard.empty.filtered': 'No results match the current filters.',
    'dashboard.method':
      'A Skill call is counted when a tool successfully reads its SKILL.md; repeated reads in the same turn count once. Status, time, and tokens use the containing turn.',
    'dashboard.sync.not_started': 'Waiting to scan',
    'dashboard.sync.syncing': 'Scanning local data',
    'dashboard.sync.ready': 'Watching for changes',
    'dashboard.sync.error': 'Sync error',
    'dashboard.sync.unavailable': 'Codex data unavailable',
    'dashboard.sync.processed': '{count} records processed',
    'dashboard.sync.updated': 'Updated {date}',
    'dashboard.status.succeeded': 'Succeeded',
    'dashboard.status.failed': 'Failed',
    'dashboard.status.cancelled': 'Cancelled',
    'dashboard.status.active': 'Active',
    'dashboard.status.unknown': 'Unknown',
    'dashboard.duration.seconds': '{value}s',
    'dashboard.duration.minutes': '{value}m',
    'dashboard.duration.hours': '{value}h',
    'dashboard.duration.days': '{value}d',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'Measure and improve AI coding workflows.',
    'about.version': 'Version',
    'about.unknown': 'Unknown',
    'about.star': 'Star on GitHub',
    'about.copyright': 'Copyright © 2026 HarnessLens',
    'about.close': 'Close',

    // Control Bar
    'control.update_available': 'Update available: v{version}',
    'control.theme_light': 'Switch to Light Theme',
    'control.theme_dark': 'Switch to Dark Theme',
    'control.settings': 'Settings',
    'control.pin': 'Pin Window',
    'control.unpin': 'Unpin Window',

    // Tray Menu
    'tray.show_main': 'Show HarnessLens',
    'tray.settings': 'Settings',
    'tray.quit': 'Quit',

    // Settings General
    'settings.general.title': 'General Settings',
    'settings.general.auto_check': 'Check for Updates Automatically',
    'settings.general.auto_check_desc': 'Check for new versions of the application upon startup',
    'settings.general.app_update': 'Application Update',

    // Settings Appearance
    'settings.appearance.title': 'App Theme',
    'settings.appearance.select': 'Theme Selection',
    'settings.appearance.desc': 'Choose between System default, Light mode, or Dark mode',
    'settings.appearance.theme_system': 'System Default',
    'settings.appearance.theme_light': 'Light Mode',
    'settings.appearance.theme_dark': 'Dark Mode',

    // Settings Language
    'settings.language.title': 'App Language',
    'settings.language.select': 'Language Selection',
    'settings.language.desc': 'Choose between System default, English, or other languages',
    'settings.language.lang_system': 'System Default',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': 'Checking for updates...',
    'update.status.installing': 'Installing update...',
    'update.status.ready': 'Update installed successfully. Please restart.',
    'update.status.error': 'Update failed: {error}',
    'update.status.available_prefix': 'New version ',
    'update.status.available_suffix': ' is available (Current: v{currentVersion})',
    'update.status.latest': 'Up to date (v{currentVersion})',

    // Update Action Buttons
    'update.action.restart': 'Restart App',
    'update.action.install': 'Install Update',
    'update.action.checking': 'Checking...',
    'update.action.check_now': 'Check now',

    // Restart Modal
    'modal.restart.title': 'Restart Required',
    'modal.restart.desc':
      'The application has been successfully updated. Restart now to apply the changes?',
    'modal.restart.later': 'Restart Later',
    'modal.restart.now': 'Restart Now',
  },
  zh: {
    // Shared / Navigation
    'nav.back': '返回分析面板',
    'nav.general': '通用设置',
    'nav.appearance': '外观设置',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      'HarnessLens是AI Coding Harness的使用效能分析工具，为Harness改进提供直观的数据依据。',

    // Analytics Dashboard
    'dashboard.eyebrow': '本机 Codex 分析',
    'dashboard.title': 'Coding Agent 分析',
    'dashboard.subtitle': '分析本机会话、轮次、Token 用量和 Skill 调用。',
    'dashboard.refresh': '刷新分析数据',
    'dashboard.loading': '正在加载本机分析数据…',
    'dashboard.summary.label': '分析概览',
    'dashboard.summary.sessions': '会话数',
    'dashboard.summary.tokens': '会话 Token',
    'dashboard.summary.skills': 'Skill 调用',
    'dashboard.turns.label': '轮次结果',
    'dashboard.turns.succeeded': '成功 {count}',
    'dashboard.turns.failed': '失败 {count}',
    'dashboard.turns.cancelled': '取消 {count}',
    'dashboard.turns.active': '执行中 {count}',
    'dashboard.turns.unknown': '未知 {count}',
    'dashboard.views.label': '分析视图',
    'dashboard.views.sessions': '会话',
    'dashboard.views.skills': 'Skills',
    'dashboard.filter.label': '会话范围',
    'dashboard.filter.all': '全部',
    'dashboard.filter.current': '当前',
    'dashboard.filter.archived': '已归档',
    'dashboard.search.label': '搜索分析数据',
    'dashboard.search.sessions': '搜索会话或项目',
    'dashboard.search.skills': '搜索 Skill',
    'dashboard.pagination.label': '分析数据分页',
    'dashboard.pagination.previous': '上一页',
    'dashboard.pagination.next': '下一页',
    'dashboard.pagination.page': '第 {page} / {total} 页',
    'dashboard.session.name': '会话',
    'dashboard.session.updated': '最后活动',
    'dashboard.session.duration': '执行时间',
    'dashboard.session.tokens': 'Token',
    'dashboard.session.turns': '轮次',
    'dashboard.session.skills': 'Skill',
    'dashboard.session.status': '最近结果',
    'dashboard.session.untitled': '未命名会话',
    'dashboard.session.unknown_project': '未知项目',
    'dashboard.session.wall_duration': '从创建到最后活动的跨度：{duration}',
    'dashboard.skill.name': 'Skill',
    'dashboard.skill.invocations': '调用次数',
    'dashboard.skill.outcomes': '成功 / 失败 / 取消 / 其他',
    'dashboard.skill.outcome_values':
      '成功 {succeeded}，失败 {failed}，取消 {cancelled}，执行中或未知 {unknown}',
    'dashboard.skill.success_rate': '成功率',
    'dashboard.skill.average_duration': '平均耗时',
    'dashboard.skill.max_duration': '最长耗时',
    'dashboard.skill.average_tokens': '平均 Token',
    'dashboard.skill.max_tokens': '最大 Token',
    'dashboard.empty.sessions': '尚未导入 Codex 会话。',
    'dashboard.empty.skills': '未检测到 Skill 调用。',
    'dashboard.empty.filtered': '没有符合当前筛选条件的结果。',
    'dashboard.method':
      '工具成功读取 SKILL.md 时记为一次 Skill 调用，同一轮重复读取只统计一次。状态、耗时和 Token 使用所在轮次的数据。',
    'dashboard.sync.not_started': '等待扫描',
    'dashboard.sync.syncing': '正在扫描本机数据',
    'dashboard.sync.ready': '正在监听数据变化',
    'dashboard.sync.error': '同步失败',
    'dashboard.sync.unavailable': 'Codex 数据不可用',
    'dashboard.sync.processed': '已处理 {count} 条记录',
    'dashboard.sync.updated': '更新于 {date}',
    'dashboard.status.succeeded': '成功',
    'dashboard.status.failed': '失败',
    'dashboard.status.cancelled': '已取消',
    'dashboard.status.active': '执行中',
    'dashboard.status.unknown': '未知',
    'dashboard.duration.seconds': '{value} 秒',
    'dashboard.duration.minutes': '{value} 分钟',
    'dashboard.duration.hours': '{value} 小时',
    'dashboard.duration.days': '{value} 天',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': '度量并改进 AI Coding 工作流。',
    'about.version': '版本',
    'about.unknown': '未知',
    'about.star': '去 GitHub 点赞',
    'about.copyright': '版权所有 © 2026 HarnessLens',
    'about.close': '关闭',

    // Control Bar
    'control.update_available': '有可用更新: v{version}',
    'control.theme_light': '切换为浅色主题',
    'control.theme_dark': '切换为深色主题',
    'control.settings': '设置',
    'control.pin': '置顶窗口',
    'control.unpin': '取消置顶窗口',

    // Tray Menu
    'tray.show_main': '显示窗口',
    'tray.settings': '设置',
    'tray.quit': '退出',

    // Settings General
    'settings.general.title': '通用设置',
    'settings.general.auto_check': '自动检查更新',
    'settings.general.auto_check_desc': '启动应用时自动检查是否有新版本',
    'settings.general.app_update': '应用更新',

    // Settings Appearance
    'settings.appearance.title': '应用主题',
    'settings.appearance.select': '选择主题',
    'settings.appearance.desc': '在系统默认、浅色模式或深色模式之间选择',
    'settings.appearance.theme_system': '系统默认',
    'settings.appearance.theme_light': '浅色模式',
    'settings.appearance.theme_dark': '深色模式',

    // Settings Language
    'settings.language.title': '应用语言',
    'settings.language.select': '语言选择',
    'settings.language.desc': '在系统默认、英文或其他语言之间选择',
    'settings.language.lang_system': '系统默认',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': '正在检查更新...',
    'update.status.installing': '正在安装更新...',
    'update.status.ready': '更新已成功安装，请重启应用。',
    'update.status.error': '更新失败: {error}',
    'update.status.available_prefix': '新版本 ',
    'update.status.available_suffix': ' 已可用（当前版本：v{currentVersion}）',
    'update.status.latest': '已是最新版本（v{currentVersion}）',

    // Update Action Buttons
    'update.action.restart': '重启应用',
    'update.action.install': '安装更新',
    'update.action.checking': '正在检查...',
    'update.action.check_now': '立即检查',

    // Restart Modal
    'modal.restart.title': '需要重启',
    'modal.restart.desc': '应用已成功更新。现在重启以应用更改吗？',
    'modal.restart.later': '稍后重启',
    'modal.restart.now': '现在重启',
  },
  zh_tw: {
    // Shared / Navigation
    'nav.back': '返回分析面板',
    'nav.general': '通用設定',
    'nav.appearance': '外觀設定',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      'HarnessLens是AI Coding Harness的使用效能分析工具，為Harness改進提供直觀的數據依據。',

    // Analytics Dashboard
    'dashboard.eyebrow': '本機 Codex 分析',
    'dashboard.title': 'Coding Agent 分析',
    'dashboard.subtitle': '分析本機工作階段、輪次、Token 用量和 Skill 呼叫。',
    'dashboard.refresh': '重新整理分析資料',
    'dashboard.loading': '正在載入本機分析資料…',
    'dashboard.summary.label': '分析概覽',
    'dashboard.summary.sessions': '工作階段數',
    'dashboard.summary.tokens': '工作階段 Token',
    'dashboard.summary.skills': 'Skill 呼叫',
    'dashboard.turns.label': '輪次結果',
    'dashboard.turns.succeeded': '成功 {count}',
    'dashboard.turns.failed': '失敗 {count}',
    'dashboard.turns.cancelled': '取消 {count}',
    'dashboard.turns.active': '執行中 {count}',
    'dashboard.turns.unknown': '未知 {count}',
    'dashboard.views.label': '分析檢視',
    'dashboard.views.sessions': '工作階段',
    'dashboard.views.skills': 'Skills',
    'dashboard.filter.label': '工作階段範圍',
    'dashboard.filter.all': '全部',
    'dashboard.filter.current': '目前',
    'dashboard.filter.archived': '已封存',
    'dashboard.search.label': '搜尋分析資料',
    'dashboard.search.sessions': '搜尋工作階段或專案',
    'dashboard.search.skills': '搜尋 Skill',
    'dashboard.pagination.label': '分析資料分頁',
    'dashboard.pagination.previous': '上一頁',
    'dashboard.pagination.next': '下一頁',
    'dashboard.pagination.page': '第 {page} / {total} 頁',
    'dashboard.session.name': '工作階段',
    'dashboard.session.updated': '最後活動',
    'dashboard.session.duration': '執行時間',
    'dashboard.session.tokens': 'Token',
    'dashboard.session.turns': '輪次',
    'dashboard.session.skills': 'Skill',
    'dashboard.session.status': '最近結果',
    'dashboard.session.untitled': '未命名工作階段',
    'dashboard.session.unknown_project': '未知專案',
    'dashboard.session.wall_duration': '從建立到最後活動的跨度：{duration}',
    'dashboard.skill.name': 'Skill',
    'dashboard.skill.invocations': '呼叫次數',
    'dashboard.skill.outcomes': '成功 / 失敗 / 取消 / 其他',
    'dashboard.skill.outcome_values':
      '成功 {succeeded}，失敗 {failed}，取消 {cancelled}，執行中或未知 {unknown}',
    'dashboard.skill.success_rate': '成功率',
    'dashboard.skill.average_duration': '平均耗時',
    'dashboard.skill.max_duration': '最長耗時',
    'dashboard.skill.average_tokens': '平均 Token',
    'dashboard.skill.max_tokens': '最大 Token',
    'dashboard.empty.sessions': '尚未匯入 Codex 工作階段。',
    'dashboard.empty.skills': '未偵測到 Skill 呼叫。',
    'dashboard.empty.filtered': '沒有符合目前篩選條件的結果。',
    'dashboard.method':
      '工具成功讀取 SKILL.md 時記為一次 Skill 呼叫，同一輪重複讀取只統計一次。狀態、耗時和 Token 使用所在輪次的資料。',
    'dashboard.sync.not_started': '等待掃描',
    'dashboard.sync.syncing': '正在掃描本機資料',
    'dashboard.sync.ready': '正在監聽資料變更',
    'dashboard.sync.error': '同步失敗',
    'dashboard.sync.unavailable': 'Codex 資料不可用',
    'dashboard.sync.processed': '已處理 {count} 筆記錄',
    'dashboard.sync.updated': '更新於 {date}',
    'dashboard.status.succeeded': '成功',
    'dashboard.status.failed': '失敗',
    'dashboard.status.cancelled': '已取消',
    'dashboard.status.active': '執行中',
    'dashboard.status.unknown': '未知',
    'dashboard.duration.seconds': '{value} 秒',
    'dashboard.duration.minutes': '{value} 分鐘',
    'dashboard.duration.hours': '{value} 小時',
    'dashboard.duration.days': '{value} 天',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': '度量並改進 AI Coding 工作流。',
    'about.version': '版本',
    'about.unknown': '未知',
    'about.star': '去 GitHub 點贊',
    'about.copyright': '版權所有 © 2026 HarnessLens',
    'about.close': '關閉',

    // Control Bar
    'control.update_available': '有可用更新: v{version}',
    'control.theme_light': '切換為淺色主題',
    'control.theme_dark': '切換為深色主題',
    'control.settings': '設定',
    'control.pin': '置頂視窗',
    'control.unpin': '取消置頂視窗',

    // Tray Menu
    'tray.show_main': '顯示 HarnessLens',
    'tray.settings': '設定...',
    'tray.quit': '結束 HarnessLens',

    // Settings General
    'settings.general.title': '通用設定',
    'settings.general.auto_check': '自動檢查更新',
    'settings.general.auto_check_desc': '啟動應用時自動檢查是否有新版本',
    'settings.general.app_update': '應用更新',

    // Settings Appearance
    'settings.appearance.title': '應用主題',
    'settings.appearance.select': '選擇主題',
    'settings.appearance.desc': '在系統預設、淺色模式或深色模式之間選擇',
    'settings.appearance.theme_system': '系統預設',
    'settings.appearance.theme_light': '淺色模式',
    'settings.appearance.theme_dark': '深色模式',

    // Settings Language
    'settings.language.title': '應用語言',
    'settings.language.select': '語言選擇',
    'settings.language.desc': '在系統預設、英文或其他語言之間選擇',
    'settings.language.lang_system': '系統預設',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': '正在檢查更新...',
    'update.status.installing': '正在安裝更新...',
    'update.status.ready': '更新已成功安裝，請重啟應用。',
    'update.status.error': '更新失敗: {error}',
    'update.status.available_prefix': '新版本 ',
    'update.status.available_suffix': ' 已可用（當前版本：v{currentVersion}）',
    'update.status.latest': '已是最新版本（v{currentVersion}）',

    // Update Action Buttons
    'update.action.restart': '重啟應用',
    'update.action.install': '安裝更新',
    'update.action.checking': '正在檢查...',
    'update.action.check_now': '立即檢查',

    // Restart Modal
    'modal.restart.title': '需要重啟',
    'modal.restart.desc': '應用已成功更新。現在重啟以套用變更嗎？',
    'modal.restart.later': '稍後重啟',
    'modal.restart.now': '現在重啟',
  },
  ja: {
    // Shared / Navigation
    'nav.back': '分析に戻る',
    'nav.general': '一般設定',
    'nav.appearance': '外観設定',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      'HarnessLensはAI Coding Harnessの利用効率を分析し、Harness改善のための分かりやすいデータ根拠を提供するツールです。',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'AI Coding ワークフローを測定し、改善します。',
    'about.version': 'バージョン',
    'about.unknown': '不明',
    'about.star': 'GitHub でスター',
    'about.copyright': 'Copyright © 2026 HarnessLens',
    'about.close': '閉じる',

    // Control Bar
    'control.update_available': 'アップデートがあります: v{version}',
    'control.theme_light': 'ライトテーマに切り替え',
    'control.theme_dark': 'ダークテーマに切り替え',
    'control.settings': '設定',
    'control.pin': 'ウィンドウを固定',
    'control.unpin': 'ウィンドウの固定を解除',

    // Tray Menu
    'tray.show_main': 'HarnessLensを表示',
    'tray.settings': '設定...',
    'tray.quit': 'HarnessLensを終了',

    // Settings General
    'settings.general.title': '一般設定',
    'settings.general.auto_check': 'アップデートを自動的に確認',
    'settings.general.auto_check_desc': '起動時に新しいバージョンを自動的に確認します',
    'settings.general.app_update': 'アプリのアップデート',

    // Settings Appearance
    'settings.appearance.title': 'テーマ',
    'settings.appearance.select': 'テーマの選択',
    'settings.appearance.desc': 'システムデフォルト、ライトモード、ダークモードから選択します',
    'settings.appearance.theme_system': 'システムデフォルト',
    'settings.appearance.theme_light': 'ライトモード',
    'settings.appearance.theme_dark': 'ダークモード',

    // Settings Language
    'settings.language.title': '言語',
    'settings.language.select': '言語の選択',
    'settings.language.desc': 'システムデフォルト、英語、日本語、その他の言語から選択します',
    'settings.language.lang_system': 'システムデフォルト',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': 'アップデートを確認中...',
    'update.status.installing': 'アップデートをインストール中...',
    'update.status.ready': 'アップデートが正常に完了しました。再起動してください。',
    'update.status.error': 'アップデート失敗: {error}',
    'update.status.available_prefix': '新バージョン ',
    'update.status.available_suffix': ' が利用可能です (現在のバージョン: v{currentVersion})',
    'update.status.latest': '最新版です (v{currentVersion})',

    // Update Action Buttons
    'update.action.restart': 'アプリを再起動',
    'update.action.install': 'アップデートをインストール',
    'update.action.checking': '確認中...',
    'update.action.check_now': '今すぐ確認',

    // Restart Modal
    'modal.restart.title': '再起動が必要',
    'modal.restart.desc': 'アプリケーションが更新されました。今すぐ再起動して変更を適用しますか？',
    'modal.restart.later': '後で再起動',
    'modal.restart.now': '今すぐ再起動',
  },
  ko: {
    // Shared / Navigation
    'nav.back': '분석으로 돌아가기',
    'nav.general': '일반 설정',
    'nav.appearance': '화면 설정',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      'HarnessLens는 AI 코딩 하니스의 사용 효율을 분석하고 하니스 개선을 위한 직관적인 데이터 근거를 제공하는 도구입니다.',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'AI 코딩 워크플로를 측정하고 개선합니다.',
    'about.version': '버전',
    'about.unknown': '알 수 없음',
    'about.star': 'GitHub 스타하기',
    'about.copyright': 'Copyright © 2026 HarnessLens',
    'about.close': '닫기',

    // Control Bar
    'control.update_available': '업데이트 가능: v{version}',
    'control.theme_light': '라이트 테마로 전환',
    'control.theme_dark': '다크 테마로 전환',
    'control.settings': '설정',
    'control.pin': '창 고정',
    'control.unpin': '창 고정 해제',

    // Tray Menu
    'tray.show_main': 'HarnessLens 표시',
    'tray.settings': '설정...',
    'tray.quit': 'HarnessLens 종료',

    // Settings General
    'settings.general.title': '일반 설정',
    'settings.general.auto_check': '업데이트 자동 확인',
    'settings.general.auto_check_desc': '시작할 때 새로운 버전을 자동으로 확인합니다',
    'settings.general.app_update': '애플리케이션 업데이트',

    // Settings Appearance
    'settings.appearance.title': '앱 테마',
    'settings.appearance.select': '테마 선택',
    'settings.appearance.desc': '템플릿 기본, 라이트 모드, 다크 모드 중에서 선택합니다',
    'settings.appearance.theme_system': '시스템 기본',
    'settings.appearance.theme_light': '라이트 모드',
    'settings.appearance.theme_dark': '다크 모드',

    // Settings Language
    'settings.language.title': '앱 언어',
    'settings.language.select': '언어 선택',
    'settings.language.desc': '시스템 기본, 영어, 한국어 또는 기타 언어 중에서 선택합니다',
    'settings.language.lang_system': '시스템 기본',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': '업데이트 확인 중...',
    'update.status.installing': '업데이트 설치 중...',
    'update.status.ready': '업데이트가 완료되었습니다. 재시작해 주세요.',
    'update.status.error': '업데이트 실패: {error}',
    'update.status.available_prefix': '새로운 버전 ',
    'update.status.available_suffix': '을 사용할 수 있습니다 (현재 버전: v{currentVersion})',
    'update.status.latest': '최신 상태입니다 (v{currentVersion})',

    // Update Action Buttons
    'update.action.restart': '앱 재시작',
    'update.action.install': '업데이트 설치',
    'update.action.checking': '확인 중...',
    'update.action.check_now': '지금 확인',

    // Restart Modal
    'modal.restart.title': '재시작 필요',
    'modal.restart.desc':
      '애플리케이션이 성공적으로 업데이트되었습니다. 지금 재시작하여 변경 사항을 적용하시겠습니까?',
    'modal.restart.later': '나중에 재시작',
    'modal.restart.now': '지금 재시작',
  },
  es: {
    // Shared / Navigation
    'nav.back': 'Volver a análisis',
    'nav.general': 'General',
    'nav.appearance': 'Apariencia',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      'HarnessLens es una herramienta de análisis de la eficacia de uso de los harnesses de programación con IA que aporta datos claros para mejorarlos.',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'Mide y mejora los flujos de trabajo de coding con IA.',
    'about.version': 'Versión',
    'about.unknown': 'Desconocido',
    'about.star': 'Destacar en GitHub',
    'about.copyright': 'Copyright © 2026 HarnessLens',
    'about.close': 'Cerrar',

    // Control Bar
    'control.update_available': 'Actualización disponible: v{version}',
    'control.theme_light': 'Cambiar a tema claro',
    'control.theme_dark': 'Cambiar a tema oscuro',
    'control.settings': 'Configuración',
    'control.pin': 'Fijar ventana',
    'control.unpin': 'Desfijar ventana',

    // Tray Menu
    'tray.show_main': 'Mostrar HarnessLens',
    'tray.settings': 'Configuración...',
    'tray.quit': 'Salir de HarnessLens',

    // Settings General
    'settings.general.title': 'Configuración General',
    'settings.general.auto_check': 'Buscar actualizaciones automáticamente',
    'settings.general.auto_check_desc': 'Buscar nuevas versiones de la aplicación al iniciar',
    'settings.general.app_update': 'Actualización de la aplicación',

    // Settings Appearance
    'settings.appearance.title': 'Tema de la aplicación',
    'settings.appearance.select': 'Selección de tema',
    'settings.appearance.desc': 'Elegir entre predeterminado del sistema, modo claro o modo oscuro',
    'settings.appearance.theme_system': 'Sistema predeterminado',
    'settings.appearance.theme_light': 'Modo claro',
    'settings.appearance.theme_dark': 'Modo oscuro',

    // Settings Language
    'settings.language.title': 'Idioma de la aplicación',
    'settings.language.select': 'Selección de idioma',
    'settings.language.desc':
      'Elegir entre predeterminado del sistema, inglés, español u otros idiomas',
    'settings.language.lang_system': 'Sistema predeterminado',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': 'Buscando actualizaciones...',
    'update.status.installing': 'Instalando actualización...',
    'update.status.ready': 'Actualización instalada con éxito. Por favor reinicie.',
    'update.status.error': 'Actualización fallida: {error}',
    'update.status.available_prefix': 'Nueva versión ',
    'update.status.available_suffix': ' disponible (Actual: v{currentVersion})',
    'update.status.latest': 'Actualizado (v{currentVersion})',

    // Update Action Buttons
    'update.action.restart': 'Reiniciar aplicación',
    'update.action.install': 'Instalar actualización',
    'update.action.checking': 'Buscando...',
    'update.action.check_now': 'Buscar ahora',

    // Restart Modal
    'modal.restart.title': 'Reinicio requerido',
    'modal.restart.desc':
      'La aplicación se ha actualizado correctamente. ¿Reiniciar ahora para aplicar los cambios?',
    'modal.restart.later': 'Reiniciar más tarde',
    'modal.restart.now': 'Reiniciar ahora',
  },
  fr: {
    // Shared / Navigation
    'nav.back': 'Retour aux analyses',
    'nav.general': 'Général',
    'nav.appearance': 'Apparence',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      "HarnessLens est un outil d'analyse de l'efficacité d'utilisation des harnesses de coding assisté par IA, fournissant des données claires pour les améliorer.",

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'Mesurez et améliorez les workflows de coding assisté par IA.',
    'about.version': 'Version',
    'about.unknown': 'Inconnu',
    'about.star': 'Ajouter une étoile sur GitHub',
    'about.copyright': 'Copyright © 2026 HarnessLens',
    'about.close': 'Fermer',

    // Control Bar
    'control.update_available': 'Mise à jour disponible : v{version}',
    'control.theme_light': 'Passer au thème clair',
    'control.theme_dark': 'Passer au thème sombre',
    'control.settings': 'Paramètres',
    'control.pin': 'Épingler la fenêtre',
    'control.unpin': 'Désélectionner la fenêtre',

    // Tray Menu
    'tray.show_main': 'Afficher HarnessLens',
    'tray.settings': 'Paramètres...',
    'tray.quit': 'Quitter HarnessLens',

    // Settings General
    'settings.general.title': 'Paramètres généraux',
    'settings.general.auto_check': 'Vérifier automatiquement les mises à jour',
    'settings.general.auto_check_desc': 'Vérifier les nouvelles versions au démarrage',
    'settings.general.app_update': "Mise à jour de l'application",

    // Settings Appearance
    'settings.appearance.title': "Thème de l'application",
    'settings.appearance.select': 'Sélection du thème',
    'settings.appearance.desc':
      'Choisir entre le thème système par défaut, le mode clair ou sombre',
    'settings.appearance.theme_system': 'Système par défaut',
    'settings.appearance.theme_light': 'Mode clair',
    'settings.appearance.theme_dark': 'Mode sombre',

    // Settings Language
    'settings.language.title': "Langue de l'application",
    'settings.language.select': 'Sélection de la langue',
    'settings.language.desc':
      "Choisir entre la langue système par défaut, l'anglais, le français ou d'autres langues",
    'settings.language.lang_system': 'Système par défaut',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': 'Recherche de mises à jour...',
    'update.status.installing': 'Installation de la mise à jour...',
    'update.status.ready': 'Mise à jour installée avec succès. Veuillez redémarrer.',
    'update.status.error': 'Échec de la mise à jour : {error}',
    'update.status.available_prefix': 'Nouvelle version ',
    'update.status.available_suffix': ' disponible (Actuelle : v{currentVersion})',
    'update.status.latest': 'À jour (v{currentVersion})',

    // Update Action Buttons
    'update.action.restart': "Redémarrer l'application",
    'update.action.install': 'Installer la mise à jour',
    'update.action.checking': 'Recherche...',
    'update.action.check_now': 'Vérifier maintenant',

    // Restart Modal
    'modal.restart.title': 'Redémarrage requis',
    'modal.restart.desc':
      "L'application a été mise à jour avec succès. Redémarrer maintenant pour appliquer les changements ?",
    'modal.restart.later': 'Plus tard',
    'modal.restart.now': 'Redémarrer maintenant',
  },
  de: {
    // Shared / Navigation
    'nav.back': 'Zurück zu Analytics',
    'nav.general': 'Allgemein',
    'nav.appearance': 'Aussehen',

    // Main Page
    'main.title': 'HarnessLens',
    'main.desc':
      'HarnessLens ist ein Werkzeug zur Analyse der Nutzungseffizienz von AI-Coding-Harnesses und liefert anschauliche Daten für deren Verbesserung.',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'AI-Coding-Workflows messen und verbessern.',
    'about.version': 'Version',
    'about.unknown': 'Unbekannt',
    'about.star': 'Stern auf GitHub geben',
    'about.copyright': 'Copyright © 2026 HarnessLens',
    'about.close': 'Schließen',

    // Control Bar
    'control.update_available': 'Update verfügbar: v{version}',
    'control.theme_light': 'Zu hellem Design wechseln',
    'control.theme_dark': 'Zu dunklem Design wechseln',
    'control.settings': 'Einstellungen',
    'control.pin': 'Fenster anheften',
    'control.unpin': 'Fenster lösen',

    // Tray Menu
    'tray.show_main': 'HarnessLens anzeigen',
    'tray.settings': 'Einstellungen...',
    'tray.quit': 'HarnessLens beenden',

    // Settings General
    'settings.general.title': 'Allgemeine Einstellungen',
    'settings.general.auto_check': 'Automatisch nach Updates suchen',
    'settings.general.auto_check_desc': 'Beim Systemstart nach neuen Versionen suchen',
    'settings.general.app_update': 'Anwendungs-Update',

    // Settings Appearance
    'settings.appearance.title': 'Design der Anwendung',
    'settings.appearance.select': 'Design-Auswahl',
    'settings.appearance.desc':
      'Wählen Sie zwischen Systemstandard, hellem Modus oder dunklem Modus',
    'settings.appearance.theme_system': 'Systemstandard',
    'settings.appearance.theme_light': 'Heller Modus',
    'settings.appearance.theme_dark': 'Dunkler Modus',

    // Settings Language
    'settings.language.title': 'Sprache der Anwendung',
    'settings.language.select': 'Sprachauswahl',
    'settings.language.desc':
      'Wählen Sie zwischen Systemstandard, Englisch, Deutsch oder anderen Sprachen',
    'settings.language.lang_system': 'Systemstandard',
    'settings.language.lang_en': 'English',
    'settings.language.lang_zh': '简体中文',
    'settings.language.lang_zh_tw': '繁體中文',
    'settings.language.lang_ja': '日本語',
    'settings.language.lang_ko': '한국어',
    'settings.language.lang_es': 'Español',
    'settings.language.lang_fr': 'Français',
    'settings.language.lang_de': 'Deutsch',

    // Update Statuses
    'update.status.checking': 'Suche nach Updates...',
    'update.status.installing': 'Update wird installiert...',
    'update.status.ready': 'Update erfolgreich installiert. Bitte neu starten.',
    'update.status.error': 'Update fehlgeschlagen: {error}',
    'update.status.available_prefix': 'Neue Version ',
    'update.status.available_suffix': ' ist verfügbar (Aktuell: v{currentVersion})',
    'update.status.latest': 'Aktuell (v{currentVersion})',

    // Update Action Buttons
    'update.action.restart': 'App neu starten',
    'update.action.install': 'Update installieren',
    'update.action.checking': 'Prüfung...',
    'update.action.check_now': 'Jetzt prüfen',

    // Restart Modal
    'modal.restart.title': 'Neustart erforderlich',
    'modal.restart.desc':
      'Die Anwendung wurde erfolgreich aktualisiert. Jetzt neu starten, um die Änderungen zu übernehmen?',
    'modal.restart.later': 'Später neu starten',
    'modal.restart.now': 'Jetzt neu starten',
  },
};

function getSystemLanguage(): ResolvedLanguage {
  if (typeof navigator === 'undefined') {
    return 'en';
  }
  const lang = (navigator.language || navigator.languages?.[0] || 'en').toLowerCase();
  if (lang.startsWith('zh-tw') || lang.startsWith('zh-hk') || lang.startsWith('zh-mo')) {
    return 'zh_tw';
  }
  if (lang.startsWith('zh')) {
    return 'zh';
  }
  if (lang.startsWith('ja')) {
    return 'ja';
  }
  if (lang.startsWith('ko')) {
    return 'ko';
  }
  if (lang.startsWith('es')) {
    return 'es';
  }
  if (lang.startsWith('fr')) {
    return 'fr';
  }
  if (lang.startsWith('de')) {
    return 'de';
  }
  return 'en';
}

class I18nManager {
  #language = $state<Language>('system');

  get language() {
    return this.#language;
  }

  set language(value: Language) {
    this.#language = value;
    saveSetting('language', value);
  }

  resolvedLanguage = $derived.by<ResolvedLanguage>(() => {
    if (this.language === 'system') {
      return getSystemLanguage();
    }
    return this.language;
  });

  async init() {
    const savedLang = (await loadSettings()).language as Language | undefined;
    if (
      savedLang === 'en' ||
      savedLang === 'zh' ||
      savedLang === 'zh_tw' ||
      savedLang === 'ja' ||
      savedLang === 'ko' ||
      savedLang === 'es' ||
      savedLang === 'fr' ||
      savedLang === 'de' ||
      savedLang === 'system'
    ) {
      this.#language = savedLang;
    }
  }

  setLanguage(lang: Language) {
    this.language = lang;
  }

  t(key: string, data?: Record<string, string | number>): string {
    const lang = this.resolvedLanguage;
    const dict = dictionaries[lang];
    let text = dict[key] ?? dictionaries.en[key] ?? key;

    if (data) {
      Object.entries(data).forEach(([k, v]) => {
        text = text.replace(`{${k}}`, String(v));
      });
    }
    return text;
  }
}

export const i18nManager = new I18nManager();
