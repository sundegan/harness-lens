export type Language = 'system' | 'en' | 'zh' | 'zh_tw' | 'ja' | 'ko' | 'es' | 'fr' | 'de';
type ResolvedLanguage = Exclude<Language, 'system'>;
type Dictionary = Record<string, string>;

import { loadSettings, saveSetting } from '$lib/settings';

const dictionaries: Record<ResolvedLanguage, Dictionary> = {
  en: {
    // Shared / Navigation
    'nav.general': 'General',
    'nav.appearance': 'Appearance',

    // Main Page
    'main.title': 'HarnessLens',
    'main.nav.label': 'Main navigation',
    'main.nav.sessions': 'Session browser',
    'main.nav.skills': 'Skill analysis',
    'main.nav.skills_desc': 'Trace skill usage and outcomes',
    'main.nav.settings': 'Configuration',
    'main.skip_to_content': 'Skip to workspace',

    // Session History
    'sessions.search': 'Search sessions',
    'sessions.search_placeholder': 'Search title, project, model, or branch',
    'sessions.search_action': 'Search',
    'sessions.filter.label': 'Archive status',
    'sessions.filter.all': 'All',
    'sessions.filter.current': 'Current',
    'sessions.filter.archived': 'Archived',
    'sessions.loading': 'Loading sessions…',
    'sessions.load_error': 'Sessions could not be loaded',
    'sessions.retry': 'Try again',
    'sessions.empty': 'No sessions found',
    'sessions.empty_desc': 'Session history will appear here after local data is indexed.',
    'sessions.empty_filtered': 'Try a different search or archive filter.',
    'sessions.untitled': 'Untitled session',
    'sessions.unknown_project': 'Unknown project',
    'sessions.unknown': 'Unknown',
    'sessions.column.session': 'Session',
    'sessions.column.model': 'Model',
    'sessions.column.activity': 'Activity',
    'sessions.column.updated': 'Updated',
    'sessions.column.runs': 'Runs',
    'sessions.column.tokens': 'Tokens',
    'sessions.column.status': 'Status',
    'sessions.overview.title': 'Session index',
    'sessions.overview.description': 'Normalized local coding-agent sessions',
    'sessions.overview.indexed_at': 'Indexed at {time}',
    'sessions.overview.results': 'Matching sessions',
    'sessions.overview.runs': 'Runs on this page',
    'sessions.overview.events': 'Events on this page',
    'sessions.overview.tokens': 'Tokens on this page',
    'sessions.activity.runs': 'runs',
    'sessions.activity.events': 'events',
    'sessions.activity.skills': '{count} Skill calls',
    'sessions.pagination.results': '{count} sessions',
    'sessions.detail.back': 'Back to sessions',
    'sessions.detail.loading': 'Loading session history…',
    'sessions.detail.load_error': 'Session history could not be loaded',
    'sessions.detail.not_found': 'This session is no longer available.',
    'sessions.detail.provider': 'Provider',
    'sessions.detail.updated': 'Last activity',
    'sessions.detail.events': 'Events',
    'sessions.detail.skills': 'Skills',
    'sessions.detail.session_label': 'Session record',
    'sessions.detail.timeline_title': 'Execution record',
    'sessions.detail.timeline_description':
      'Messages, tool calls, reasoning, and runtime events in source order',
    'sessions.detail.run_count': '{count} runs',
    'sessions.detail.group_items': '{count} items',
    'sessions.detail.invocation': 'Run {count}',
    'sessions.detail.session_events': 'Session events',
    'sessions.detail.no_events': 'No normalized events',
    'sessions.detail.no_events_desc':
      'Metadata is available, but no conversation or tool events were indexed for this session.',
    'sessions.detail.empty_message': 'Empty message',
    'sessions.detail.image': '[Image]',
    'sessions.detail.audio': '[Audio]',
    'sessions.detail.attachment': '[Attachment]',
    'sessions.detail.role.user': 'You',
    'sessions.detail.role.assistant': 'Assistant',
    'sessions.detail.role.developer': 'Developer instructions',
    'sessions.detail.role.system': 'System',
    'sessions.detail.role.tool': 'Tool',
    'sessions.detail.role.unknown': 'Message',
    'sessions.detail.tool_details': 'Show input and output',
    'sessions.detail.input': 'Input',
    'sessions.detail.output': 'Output',
    'sessions.detail.no_tool_result': 'No terminal tool result was recorded.',
    'sessions.detail.reasoning': 'Reasoning',
    'sessions.detail.no_reasoning': 'No visible reasoning content.',
    'sessions.detail.other_events': '{count} runtime events',
    'sessions.detail.other_events_desc':
      'Plans, context changes, approvals, and other technical events',
    'sessions.status.succeeded': 'Succeeded',
    'sessions.status.failed': 'Failed',
    'sessions.status.cancelled': 'Cancelled',
    'sessions.status.in_progress': 'In progress',
    'sessions.status.active': 'Active',
    'sessions.status.unknown': 'Unknown',
    'sessions.status.pending': 'Pending',
    'sessions.status.awaiting_approval': 'Awaiting approval',
    'sessions.status.completed': 'Completed',
    'sessions.status.declined': 'Declined',
    'sessions.event.plan': 'Plan update',
    'sessions.event.approval_request': 'Approval request',
    'sessions.event.approval_decision': 'Approval decision',
    'sessions.event.model_invocation': 'Model invocation',
    'sessions.event.agent_invocation': 'Agent invocation',
    'sessions.event.file_change': 'File change',
    'sessions.event.world_state': 'World state',
    'sessions.event.goal': 'Goal update',
    'sessions.event.fork_invocation_boundary': 'Fork boundary',
    'sessions.event.input_queue': 'Input queue',
    'sessions.event.context_compaction': 'Context compaction',
    'sessions.event.execution_context': 'Execution context',
    'sessions.event.mode_change': 'Mode change',
    'sessions.event.notice': 'Notice',
    'sessions.event.hook_result': 'Hook result',
    'sessions.event.retry': 'Retry',
    'sessions.event.rollback': 'Rollback',
    'sessions.event.unknown': 'Provider event',

    // Analytics Dashboard
    'dashboard.loading': 'Loading local analytics…',
    'dashboard.summary.skills': 'Skill calls',
    'dashboard.search.label': 'Search analytics',
    'dashboard.search.skills': 'Search skills',
    'dashboard.skill.name': 'Skill',
    'dashboard.skill.invocations': 'Runs',
    'dashboard.skill.outcomes': 'Success / fail / cancel / other',
    'dashboard.skill.outcome_values':
      '{succeeded} succeeded, {failed} failed, {cancelled} cancelled, {unknown} active or unknown',
    'dashboard.skill.success_rate': 'Success',
    'dashboard.skill.average_duration': 'Avg time',
    'dashboard.skill.average_tokens': 'Avg tokens',
    'dashboard.pagination.results': '{count} skills',
    'dashboard.empty.skills': 'No Skill calls were detected.',
    'dashboard.empty.filtered': 'No results match the current filters.',
    'dashboard.sync.error': 'Sync error',

    // Pagination
    'pagination.rows_per_page': 'Rows per page',
    'pagination.navigation': 'Pagination',
    'pagination.previous': 'Previous page',
    'pagination.next': 'Next page',
    'pagination.page_input': 'Page number',
    'pagination.go': 'Go',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'Measure and improve AI coding workflows.',
    'about.version': 'Version',
    'about.unknown': 'Unknown',
    'about.close': 'Close',

    // Control Bar
    'control.update_available': 'Update available: v{version}',
    'control.theme_light': 'Switch to Light Theme',
    'control.theme_dark': 'Switch to Dark Theme',
    'control.settings': 'Settings',
    'control.pin': 'Pin Window',
    'control.unpin': 'Unpin Window',
    'control.window_controls': 'Window controls',
    'control.minimize': 'Minimize',
    'control.maximize': 'Maximize',
    'control.restore': 'Restore',
    'control.close': 'Close',

    // Tray Menu
    'tray.show_main': 'Show HarnessLens',
    'tray.settings': 'Settings',
    'tray.quit': 'Quit',

    // Settings General
    'settings.general.title': 'General Settings',

    // Settings Appearance
    'settings.appearance.title': 'App Theme',
    'settings.appearance.select': 'Theme Selection',
    'settings.appearance.desc': 'Choose between System default, Light mode, or Dark mode',
    'settings.appearance.theme_system': 'System Default',
    'settings.appearance.theme_light': 'Light Mode',
    'settings.appearance.theme_dark': 'Dark Mode',

    // Settings Language
    'settings.language.title': 'App Language',
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
    'nav.general': '通用设置',
    'nav.appearance': '外观设置',

    // Main Page
    'main.title': 'HarnessLens',
    'main.nav.label': '主导航',
    'main.nav.sessions': '会话浏览',
    'main.nav.skills': 'Skill 分析',
    'main.nav.skills_desc': '追踪 Skill 使用和结果',
    'main.nav.settings': '配置',
    'main.skip_to_content': '跳转到工作区',

    // Session History
    'sessions.search': '搜索会话',
    'sessions.search_placeholder': '搜索标题、项目、模型或分支',
    'sessions.search_action': '搜索',
    'sessions.filter.label': '归档状态',
    'sessions.filter.all': '全部',
    'sessions.filter.current': '当前',
    'sessions.filter.archived': '已归档',
    'sessions.loading': '正在加载会话…',
    'sessions.load_error': '无法加载会话',
    'sessions.retry': '重试',
    'sessions.empty': '没有找到会话',
    'sessions.empty_desc': '本地数据完成索引后，会话历史会显示在这里。',
    'sessions.empty_filtered': '请尝试其他搜索词或归档筛选条件。',
    'sessions.untitled': '未命名会话',
    'sessions.unknown_project': '未知项目',
    'sessions.unknown': '未知',
    'sessions.column.session': '会话',
    'sessions.column.model': '模型',
    'sessions.column.activity': '活动',
    'sessions.column.updated': '最近活动',
    'sessions.column.runs': '执行',
    'sessions.column.tokens': 'Token',
    'sessions.column.status': '状态',
    'sessions.overview.title': '会话索引',
    'sessions.overview.description': '基于本地数据规范化后的 Coding Agent 会话',
    'sessions.overview.indexed_at': '{time} 完成索引',
    'sessions.overview.results': '匹配会话',
    'sessions.overview.runs': '本页执行',
    'sessions.overview.events': '本页事件',
    'sessions.overview.tokens': '本页 Token',
    'sessions.activity.runs': '次执行',
    'sessions.activity.events': '条事件',
    'sessions.activity.skills': '{count} 次 Skill 调用',
    'sessions.pagination.results': '共 {count} 个会话',
    'sessions.detail.back': '返回会话列表',
    'sessions.detail.loading': '正在加载会话记录…',
    'sessions.detail.load_error': '无法加载会话记录',
    'sessions.detail.not_found': '该会话已不存在。',
    'sessions.detail.provider': '数据源',
    'sessions.detail.updated': '最近活动',
    'sessions.detail.events': '事件',
    'sessions.detail.skills': 'Skill',
    'sessions.detail.session_label': '会话记录',
    'sessions.detail.timeline_title': '执行记录',
    'sessions.detail.timeline_description': '按原始顺序展示消息、工具调用、推理和运行事件',
    'sessions.detail.run_count': '共 {count} 次执行',
    'sessions.detail.group_items': '{count} 条记录',
    'sessions.detail.invocation': '第 {count} 次执行',
    'sessions.detail.session_events': '会话级事件',
    'sessions.detail.no_events': '暂无规范化事件',
    'sessions.detail.no_events_desc': '会话元数据已收录，但还没有索引到对话或工具事件。',
    'sessions.detail.empty_message': '空消息',
    'sessions.detail.image': '[图片]',
    'sessions.detail.audio': '[音频]',
    'sessions.detail.attachment': '[附件]',
    'sessions.detail.role.user': '你',
    'sessions.detail.role.assistant': '助手',
    'sessions.detail.role.developer': '开发者指令',
    'sessions.detail.role.system': '系统',
    'sessions.detail.role.tool': '工具',
    'sessions.detail.role.unknown': '消息',
    'sessions.detail.tool_details': '查看输入和输出',
    'sessions.detail.input': '输入',
    'sessions.detail.output': '输出',
    'sessions.detail.no_tool_result': '没有记录到工具的最终结果。',
    'sessions.detail.reasoning': '推理过程',
    'sessions.detail.no_reasoning': '没有可展示的推理内容。',
    'sessions.detail.other_events': '{count} 条运行事件',
    'sessions.detail.other_events_desc': '计划、上下文变更、授权及其他技术事件',
    'sessions.status.succeeded': '成功',
    'sessions.status.failed': '失败',
    'sessions.status.cancelled': '已取消',
    'sessions.status.in_progress': '执行中',
    'sessions.status.active': '执行中',
    'sessions.status.unknown': '未知',
    'sessions.status.pending': '等待执行',
    'sessions.status.awaiting_approval': '等待授权',
    'sessions.status.completed': '已完成',
    'sessions.status.declined': '已拒绝',
    'sessions.event.plan': '计划更新',
    'sessions.event.approval_request': '授权请求',
    'sessions.event.approval_decision': '授权结果',
    'sessions.event.model_invocation': '模型调用',
    'sessions.event.agent_invocation': 'Agent 调用',
    'sessions.event.file_change': '文件变更',
    'sessions.event.world_state': '运行状态',
    'sessions.event.goal': '目标更新',
    'sessions.event.fork_invocation_boundary': '分支调用边界',
    'sessions.event.input_queue': '输入队列',
    'sessions.event.context_compaction': '上下文压缩',
    'sessions.event.execution_context': '执行上下文',
    'sessions.event.mode_change': '模式变更',
    'sessions.event.notice': '运行通知',
    'sessions.event.hook_result': 'Hook 结果',
    'sessions.event.retry': '重试',
    'sessions.event.rollback': '回滚',
    'sessions.event.unknown': '原始事件',

    // Analytics Dashboard
    'dashboard.loading': '正在加载本机分析数据…',
    'dashboard.summary.skills': 'Skill 调用',
    'dashboard.search.label': '搜索分析数据',
    'dashboard.search.skills': '搜索 Skill',
    'dashboard.skill.name': 'Skill',
    'dashboard.skill.invocations': '调用次数',
    'dashboard.skill.outcomes': '成功 / 失败 / 取消 / 其他',
    'dashboard.skill.outcome_values':
      '成功 {succeeded}，失败 {failed}，取消 {cancelled}，执行中或未知 {unknown}',
    'dashboard.skill.success_rate': '成功率',
    'dashboard.skill.average_duration': '平均耗时',
    'dashboard.skill.average_tokens': '平均 Token',
    'dashboard.pagination.results': '共 {count} 个 Skill',
    'dashboard.empty.skills': '未检测到 Skill 调用。',
    'dashboard.empty.filtered': '没有符合当前筛选条件的结果。',
    'dashboard.sync.error': '同步失败',

    // Pagination
    'pagination.rows_per_page': '每页行数',
    'pagination.navigation': '分页导航',
    'pagination.previous': '上一页',
    'pagination.next': '下一页',
    'pagination.page_input': '页码',
    'pagination.go': '跳转',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': '度量并改进 AI Coding 工作流。',
    'about.version': '版本',
    'about.unknown': '未知',
    'about.close': '关闭',

    // Control Bar
    'control.update_available': '有可用更新: v{version}',
    'control.theme_light': '切换为浅色主题',
    'control.theme_dark': '切换为深色主题',
    'control.settings': '设置',
    'control.pin': '置顶窗口',
    'control.unpin': '取消置顶窗口',
    'control.window_controls': '窗口控制',
    'control.minimize': '最小化',
    'control.maximize': '最大化',
    'control.restore': '还原',
    'control.close': '关闭',

    // Tray Menu
    'tray.show_main': '显示窗口',
    'tray.settings': '设置',
    'tray.quit': '退出',

    // Settings General
    'settings.general.title': '通用设置',

    // Settings Appearance
    'settings.appearance.title': '应用主题',
    'settings.appearance.select': '选择主题',
    'settings.appearance.desc': '在系统默认、浅色模式或深色模式之间选择',
    'settings.appearance.theme_system': '系统默认',
    'settings.appearance.theme_light': '浅色模式',
    'settings.appearance.theme_dark': '深色模式',

    // Settings Language
    'settings.language.title': '应用语言',
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
    'nav.general': '通用設定',
    'nav.appearance': '外觀設定',

    // Main Page
    'main.title': 'HarnessLens',

    // Analytics Dashboard
    'dashboard.loading': '正在載入本機分析資料…',
    'dashboard.summary.skills': 'Skill 呼叫',
    'dashboard.search.label': '搜尋分析資料',
    'dashboard.search.skills': '搜尋 Skill',
    'dashboard.skill.name': 'Skill',
    'dashboard.skill.invocations': '呼叫次數',
    'dashboard.skill.outcomes': '成功 / 失敗 / 取消 / 其他',
    'dashboard.skill.outcome_values':
      '成功 {succeeded}，失敗 {failed}，取消 {cancelled}，執行中或未知 {unknown}',
    'dashboard.skill.success_rate': '成功率',
    'dashboard.skill.average_duration': '平均耗時',
    'dashboard.skill.average_tokens': '平均 Token',
    'dashboard.pagination.results': '共 {count} 個 Skill',
    'dashboard.empty.skills': '未偵測到 Skill 呼叫。',
    'dashboard.empty.filtered': '沒有符合目前篩選條件的結果。',
    'dashboard.sync.error': '同步失敗',

    // Pagination
    'pagination.rows_per_page': '每頁行數',
    'pagination.navigation': '分頁導覽',
    'pagination.previous': '上一頁',
    'pagination.next': '下一頁',
    'pagination.page_input': '頁碼',
    'pagination.go': '跳轉',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': '度量並改進 AI Coding 工作流。',
    'about.version': '版本',
    'about.unknown': '未知',
    'about.close': '關閉',

    // Control Bar
    'control.update_available': '有可用更新: v{version}',
    'control.theme_light': '切換為淺色主題',
    'control.theme_dark': '切換為深色主題',
    'control.settings': '設定',
    'control.pin': '置頂視窗',
    'control.unpin': '取消置頂視窗',
    'control.window_controls': '視窗控制',
    'control.minimize': '最小化',
    'control.maximize': '最大化',
    'control.restore': '還原',
    'control.close': '關閉',

    // Tray Menu
    'tray.show_main': '顯示 HarnessLens',
    'tray.settings': '設定...',
    'tray.quit': '結束 HarnessLens',

    // Settings General
    'settings.general.title': '通用設定',

    // Settings Appearance
    'settings.appearance.title': '應用主題',
    'settings.appearance.select': '選擇主題',
    'settings.appearance.desc': '在系統預設、淺色模式或深色模式之間選擇',
    'settings.appearance.theme_system': '系統預設',
    'settings.appearance.theme_light': '淺色模式',
    'settings.appearance.theme_dark': '深色模式',

    // Settings Language
    'settings.language.title': '應用語言',
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
    'nav.general': '一般設定',
    'nav.appearance': '外観設定',

    // Main Page
    'main.title': 'HarnessLens',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'AI Coding ワークフローを測定し、改善します。',
    'about.version': 'バージョン',
    'about.unknown': '不明',
    'about.close': '閉じる',

    // Control Bar
    'control.update_available': 'アップデートがあります: v{version}',
    'control.theme_light': 'ライトテーマに切り替え',
    'control.theme_dark': 'ダークテーマに切り替え',
    'control.settings': '設定',
    'control.pin': 'ウィンドウを固定',
    'control.unpin': 'ウィンドウの固定を解除',
    'control.window_controls': 'ウィンドウ操作',
    'control.minimize': '最小化',
    'control.maximize': '最大化',
    'control.restore': '元に戻す',
    'control.close': '閉じる',

    // Tray Menu
    'tray.show_main': 'HarnessLensを表示',
    'tray.settings': '設定...',
    'tray.quit': 'HarnessLensを終了',

    // Settings General
    'settings.general.title': '一般設定',

    // Settings Appearance
    'settings.appearance.title': 'テーマ',
    'settings.appearance.select': 'テーマの選択',
    'settings.appearance.desc': 'システムデフォルト、ライトモード、ダークモードから選択します',
    'settings.appearance.theme_system': 'システムデフォルト',
    'settings.appearance.theme_light': 'ライトモード',
    'settings.appearance.theme_dark': 'ダークモード',

    // Settings Language
    'settings.language.title': '言語',
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
    'nav.general': '일반 설정',
    'nav.appearance': '화면 설정',

    // Main Page
    'main.title': 'HarnessLens',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'AI 코딩 워크플로를 측정하고 개선합니다.',
    'about.version': '버전',
    'about.unknown': '알 수 없음',
    'about.close': '닫기',

    // Control Bar
    'control.update_available': '업데이트 가능: v{version}',
    'control.theme_light': '라이트 테마로 전환',
    'control.theme_dark': '다크 테마로 전환',
    'control.settings': '설정',
    'control.pin': '창 고정',
    'control.unpin': '창 고정 해제',
    'control.window_controls': '창 제어',
    'control.minimize': '최소화',
    'control.maximize': '최대화',
    'control.restore': '복원',
    'control.close': '닫기',

    // Tray Menu
    'tray.show_main': 'HarnessLens 표시',
    'tray.settings': '설정...',
    'tray.quit': 'HarnessLens 종료',

    // Settings General
    'settings.general.title': '일반 설정',

    // Settings Appearance
    'settings.appearance.title': '앱 테마',
    'settings.appearance.select': '테마 선택',
    'settings.appearance.desc': '템플릿 기본, 라이트 모드, 다크 모드 중에서 선택합니다',
    'settings.appearance.theme_system': '시스템 기본',
    'settings.appearance.theme_light': '라이트 모드',
    'settings.appearance.theme_dark': '다크 모드',

    // Settings Language
    'settings.language.title': '앱 언어',
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
    'nav.general': 'General',
    'nav.appearance': 'Apariencia',

    // Main Page
    'main.title': 'HarnessLens',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'Mide y mejora los flujos de trabajo de coding con IA.',
    'about.version': 'Versión',
    'about.unknown': 'Desconocido',
    'about.close': 'Cerrar',

    // Control Bar
    'control.update_available': 'Actualización disponible: v{version}',
    'control.theme_light': 'Cambiar a tema claro',
    'control.theme_dark': 'Cambiar a tema oscuro',
    'control.settings': 'Configuración',
    'control.pin': 'Fijar ventana',
    'control.unpin': 'Desfijar ventana',
    'control.window_controls': 'Controles de ventana',
    'control.minimize': 'Minimizar',
    'control.maximize': 'Maximizar',
    'control.restore': 'Restaurar',
    'control.close': 'Cerrar',

    // Tray Menu
    'tray.show_main': 'Mostrar HarnessLens',
    'tray.settings': 'Configuración...',
    'tray.quit': 'Salir de HarnessLens',

    // Settings General
    'settings.general.title': 'Configuración General',

    // Settings Appearance
    'settings.appearance.title': 'Tema de la aplicación',
    'settings.appearance.select': 'Selección de tema',
    'settings.appearance.desc': 'Elegir entre predeterminado del sistema, modo claro o modo oscuro',
    'settings.appearance.theme_system': 'Sistema predeterminado',
    'settings.appearance.theme_light': 'Modo claro',
    'settings.appearance.theme_dark': 'Modo oscuro',

    // Settings Language
    'settings.language.title': 'Idioma de la aplicación',
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
    'nav.general': 'Général',
    'nav.appearance': 'Apparence',

    // Main Page
    'main.title': 'HarnessLens',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'Mesurez et améliorez les workflows de coding assisté par IA.',
    'about.version': 'Version',
    'about.unknown': 'Inconnu',
    'about.close': 'Fermer',

    // Control Bar
    'control.update_available': 'Mise à jour disponible : v{version}',
    'control.theme_light': 'Passer au thème clair',
    'control.theme_dark': 'Passer au thème sombre',
    'control.settings': 'Paramètres',
    'control.pin': 'Épingler la fenêtre',
    'control.unpin': 'Désélectionner la fenêtre',
    'control.window_controls': 'Commandes de la fenêtre',
    'control.minimize': 'Réduire',
    'control.maximize': 'Agrandir',
    'control.restore': 'Restaurer',
    'control.close': 'Fermer',

    // Tray Menu
    'tray.show_main': 'Afficher HarnessLens',
    'tray.settings': 'Paramètres...',
    'tray.quit': 'Quitter HarnessLens',

    // Settings General
    'settings.general.title': 'Paramètres généraux',

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
    'nav.general': 'Allgemein',
    'nav.appearance': 'Aussehen',

    // Main Page
    'main.title': 'HarnessLens',

    // About Dialog
    'about.title': 'HarnessLens',
    'about.desc': 'AI-Coding-Workflows messen und verbessern.',
    'about.version': 'Version',
    'about.unknown': 'Unbekannt',
    'about.close': 'Schließen',

    // Control Bar
    'control.update_available': 'Update verfügbar: v{version}',
    'control.theme_light': 'Zu hellem Design wechseln',
    'control.theme_dark': 'Zu dunklem Design wechseln',
    'control.settings': 'Einstellungen',
    'control.pin': 'Fenster anheften',
    'control.unpin': 'Fenster lösen',
    'control.window_controls': 'Fenstersteuerung',
    'control.minimize': 'Minimieren',
    'control.maximize': 'Maximieren',
    'control.restore': 'Wiederherstellen',
    'control.close': 'Schließen',

    // Tray Menu
    'tray.show_main': 'HarnessLens anzeigen',
    'tray.settings': 'Einstellungen...',
    'tray.quit': 'HarnessLens beenden',

    // Settings General
    'settings.general.title': 'Allgemeine Einstellungen',

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
