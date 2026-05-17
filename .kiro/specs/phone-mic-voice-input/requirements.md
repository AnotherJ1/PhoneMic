# Requirements Document

## Introduction

本功能（代号 PhoneMic）将用户的智能手机变成电脑的"无线语音麦克风"。用户在电脑上运行一个跨平台桌面应用，该应用内置 Web 服务器；同一局域网中的手机通过浏览器访问该网页，使用手机的语音识别能力录入语音并转换为文本，文本通过桌面应用模拟键盘输入到电脑当前光标所在的位置。

本文档定义该功能的功能性与非功能性需求，覆盖跨平台支持、Web 服务、设备配对与发现、移动端 Web 界面、语音识别、键盘注入、安全性、多语言支持以及错误处理与连接稳定性等方面。

## Glossary

- **Desktop_App**: 在用户电脑上运行的跨平台桌面应用程序，作为系统的主控端，负责承载 Web 服务器、模拟键盘输入并管理与手机端的会话。
- **Web_Server**: 内置于 Desktop_App 的 HTTP/HTTPS 服务器，用于向手机端浏览器提供网页（Mobile_Web_Client）和 WebSocket 实时通信通道。
- **Mobile_Web_Client**: 手机浏览器加载的网页应用，提供录音、状态显示、转写文本预览与发送等用户界面。
- **Pairing_Service**: Desktop_App 中负责设备配对、配对码生成与校验、会话令牌签发与吊销的子模块。
- **Session_Token**: 手机端配对成功后由 Pairing_Service 颁发的、用于后续请求鉴权的令牌。
- **Discovery_Service**: 基于 mDNS/Bonshou 等机制在局域网内进行服务广播与发现的子模块。
- **ASR_Engine**: 语音转文字引擎。可为浏览器侧的 Web Speech API（Browser_ASR）或服务端 ASR（Server_ASR）。
- **Browser_ASR**: 在 Mobile_Web_Client 内运行的浏览器原生语音识别（Web Speech API）。
- **Server_ASR**: 由 Desktop_App 调用的服务端语音识别（本地或云端引擎）。
- **Input_Injector**: 桌面端的跨平台键盘输入注入子模块，负责将文本作为键盘事件发送到操作系统当前焦点位置。
- **Cursor_Target**: 电脑上当前接收键盘输入的焦点位置（如文本框、编辑器中的光标处）。
- **LAN**: 同一局域网（Local Area Network）。Desktop_App 与手机需处于同一 LAN 才能建立连接。
- **QR_Code**: 由 Desktop_App 生成、包含连接 URL 与配对码的二维码，供手机扫码连接。
- **Pairing_Code**: 用于设备配对验证的一次性数字或字符串口令。
- **Connection_Channel**: Desktop_App 与 Mobile_Web_Client 之间用于实时双向通信的 WebSocket 连接。

## Requirements

### Requirement 1: 跨平台桌面应用支持

**User Story:** 作为用户，我希望在 Windows、macOS 和 Linux 上都能运行该桌面应用，以便不论使用哪种操作系统都能使用手机麦克风输入功能。

#### Acceptance Criteria

1. THE Desktop_App SHALL 提供面向 Windows 10 及以上、macOS 11 及以上、主流 Linux 发行版（Ubuntu 20.04 及以上、Fedora 36 及以上）的安装包。
2. WHEN Desktop_App 在受支持的操作系统上启动，THE Desktop_App SHALL 在 5 秒内完成初始化并显示主界面，且在初始化完成前不展示主界面。
3. WHILE Desktop_App 处于初始化过程中，THE Desktop_App SHALL 显示包含当前阶段说明（如"正在启动 Web 服务器""正在生成证书"）与进度指示的启动画面。
4. THE Desktop_App SHALL 在所有受支持平台上提供一致的核心功能集，包括 Web_Server 启动、二维码显示、连接状态查看与文本输入注入。
5. IF 当前操作系统或版本不在受支持范围内，THEN THE Desktop_App SHALL 在启动时显示明确的不支持提示并退出。
6. THE Desktop_App SHALL 在系统托盘（Windows/Linux）或菜单栏（macOS）提供常驻图标，并支持后台运行。

### Requirement 2: 内置 Web 服务器与端口管理

**User Story:** 作为用户，我希望桌面应用启动后自动在本机开启一个 Web 服务，使我的手机能通过浏览器访问页面，而不必手动安装手机 App。

#### Acceptance Criteria

1. WHEN Desktop_App 启动完成，THE Web_Server SHALL 在本机绑定一个可用的 TCP 端口并开始监听 HTTP 请求。
2. THE Web_Server SHALL 优先使用配置文件中指定的端口；IF 该端口被占用，THEN THE Web_Server SHALL 在 1024–65535 范围内自动选择下一个可用端口。
3. THE Web_Server SHALL 同时支持 HTTP 与 HTTPS（自签名证书）两种协议，并允许用户在设置中选择启用方式。
4. WHERE 用户启用 HTTPS，THE Web_Server SHALL 在首次启动时生成自签名证书并持久化保存于用户配置目录。
5. WHEN Mobile_Web_Client 通过浏览器访问 Web_Server 的根路径，THE Web_Server SHALL 在 2 秒内返回手机端网页资源（HTML、CSS、JS）。
6. THE Web_Server SHALL 提供 WebSocket 端点用于建立 Connection_Channel。
7. WHEN Desktop_App 退出，THE Web_Server SHALL 在 3 秒内停止监听并释放占用的端口。
8. IF Web_Server 启动失败（例如全部候选端口均被占用），THEN THE Desktop_App SHALL 显示错误信息并提供重试入口。

### Requirement 3: 局域网设备发现与连接信息呈现

**User Story:** 作为用户，我希望桌面应用能清晰告诉我手机应该访问哪个地址，并通过扫码快速完成连接，避免手动输入 IP。

#### Acceptance Criteria

1. WHEN Web_Server 成功启动，THE Desktop_App SHALL 在主界面显示当前可用的连接 URL，包含本机在 LAN 中的 IPv4 地址与监听端口。
2. WHEN 主机存在多块网卡或多个 LAN IP，THE Desktop_App SHALL 列出全部可用的 LAN IPv4 地址供用户选择。
3. THE Desktop_App SHALL 根据当前选定的连接 URL 与 Pairing_Code 生成 QR_Code 并显示在主界面。
4. THE Discovery_Service SHALL 通过 mDNS 在 LAN 中以服务名称 "_phonemic._tcp" 广播 Web_Server 的主机名与端口。
5. WHEN 主机的 LAN IP 发生变化，THE Desktop_App SHALL 在 5 秒内刷新显示的连接 URL 与 QR_Code。
6. IF 检测到主机未连接到任何 LAN（例如网线未插或 Wi-Fi 未连接），THEN THE Desktop_App SHALL 显示"未检测到局域网连接"的提示并禁用扫码区域。

### Requirement 4: 手机端 Web 界面

**User Story:** 作为用户，我希望在手机上打开网页就能立刻进行语音录制，并清晰看到当前的连接状态和识别结果。

#### Acceptance Criteria

1. WHEN Mobile_Web_Client 在主流移动浏览器（iOS Safari 15 及以上、Android Chrome 100 及以上）中加载，THE Mobile_Web_Client SHALL 正确显示界面且关键控件（录音按钮、状态指示、文本预览区）均可交互。
2. THE Mobile_Web_Client SHALL 在主界面始终显示一个明显的录音按钮，并支持按住录音与点击切换录音两种模式，用户可在设置中选择当前使用的模式。
3. WHILE 录音进行中，THE Mobile_Web_Client SHALL 通过视觉指示（如波形或闪烁动画）展示录音状态。
4. WHILE 录音进行中且使用 Browser_ASR，THE Mobile_Web_Client SHALL 实时显示中间识别结果（interim transcript）。
5. THE Mobile_Web_Client SHALL 在界面上显示与 Desktop_App 之间的连接状态（未连接、已连接、重连中、断开）。
6. THE Mobile_Web_Client SHALL 提供"发送"按钮，将当前确认的转写文本通过 Connection_Channel 发送到 Desktop_App。
7. WHERE 用户启用自动发送模式，THE Mobile_Web_Client SHALL 在每段语音识别得到最终结果后自动发送文本，无需用户点击发送。
8. IF 用户拒绝授予麦克风权限，THEN THE Mobile_Web_Client SHALL 显示包含权限设置指引的错误信息。
9. THE Mobile_Web_Client SHALL 在断开连接后保留最近 50 条识别记录，直至用户主动清除或刷新页面。

### Requirement 5: 语音转文字（ASR）

**User Story:** 作为用户，我希望我的语音被准确地转换为文字，并支持多种语言（特别是中文），以便我能在不同语言场景下使用。

#### Acceptance Criteria

1. THE ASR_Engine SHALL 至少支持简体中文（zh-CN）与英文（en-US）两种识别语言。
2. THE Mobile_Web_Client SHALL 提供识别语言选择控件，允许用户在受支持语言列表中切换。
3. WHERE 移动浏览器支持 Web Speech API，THE Mobile_Web_Client SHALL 默认使用 Browser_ASR 进行语音识别。
4. WHERE 移动浏览器不支持 Web Speech API 或用户在设置中显式选择服务端识别，THE Mobile_Web_Client SHALL 通过 Connection_Channel 将音频数据发送到 Desktop_App，由 Server_ASR 完成识别。
5. WHEN ASR_Engine 完成一段识别并产出最终结果，THE ASR_Engine SHALL 将文本以 UTF-8 编码返回，且不丢失中文字符或表情符号。
6. THE ASR_Engine SHALL 在用户完成一段不超过 10 秒的语音输入后，于 3 秒内返回最终识别结果（在网络与设备性能正常的前提下）。
7. IF ASR_Engine 在 10 秒内未返回任何识别结果，THEN THE Mobile_Web_Client SHALL 显示识别超时提示并允许用户重试。
8. THE Mobile_Web_Client SHALL 提供"在发送前手动编辑文本"的入口，使用户可在文本注入前修正识别结果。

### Requirement 6: 键盘输入注入到光标位置

**User Story:** 作为用户，我希望识别后的文字直接被输入到我电脑光标所在的应用中（如浏览器输入框、文本编辑器、聊天软件等），而不是只显示在桌面应用内部。

#### Acceptance Criteria

1. WHEN Desktop_App 通过 Connection_Channel 收到来自已配对手机的文本消息，THE Input_Injector SHALL 将文本作为键盘输入事件发送到操作系统当前的输入焦点（Cursor_Target）。
2. THE Input_Injector SHALL 在 Windows、macOS 与 Linux 上分别使用各平台官方推荐的输入注入接口实现等效功能。
3. THE Input_Injector SHALL 在注入文本时保留 Unicode 字符（包括中文、emoji 与全角标点），不出现乱码或字符截断。
4. WHEN 文本中包含换行符 "\n"，THE Input_Injector SHALL 将其作为回车键事件注入。
5. THE Desktop_App SHALL 提供"注入延迟"配置项，取值范围 0–500 毫秒，默认 0；THE Input_Injector SHALL 在每个字符注入之间按该延迟等待。
6. WHEN Desktop_App 收到一条文本但当前操作系统无可识别的输入焦点（例如桌面无前台应用），THE Desktop_App SHALL 取消注入并向 Mobile_Web_Client 返回"无可注入目标"的错误。
7. THE Desktop_App SHALL 提供"暂停注入"开关；WHILE 暂停注入处于开启状态，THE Input_Injector SHALL 不执行任何键盘注入，但 Desktop_App 仍可显示已接收文本。
8. IF 用户操作系统拒绝授予输入注入所需权限（如 macOS 辅助功能权限），THEN THE Desktop_App SHALL 显示权限引导界面并暂停注入功能直至权限被授予。

### Requirement 7: 设备配对与安全访问

**User Story:** 作为用户，我希望只有经过我授权的手机能向我的电脑发送文字，避免局域网内其他设备未经允许访问。

#### Acceptance Criteria

1. WHEN Web_Server 启动，THE Pairing_Service SHALL 生成一个长度不少于 6 位、包含数字或大小写字母的 Pairing_Code，并将其编码进 QR_Code。
2. WHEN Mobile_Web_Client 首次连接到 Web_Server，THE Pairing_Service SHALL 要求其提交有效的 Pairing_Code 才能完成配对。
3. WHEN Pairing_Code 校验通过，THE Pairing_Service SHALL 颁发一个 Session_Token 给 Mobile_Web_Client，并将其与设备指纹关联保存。
4. THE Mobile_Web_Client SHALL 在后续所有 WebSocket 与 HTTP 请求中携带 Session_Token。
5. IF Mobile_Web_Client 提交的 Pairing_Code 在 5 次尝试内均不匹配，THEN THE Pairing_Service SHALL 在该客户端 IP 上对配对请求进行 5 分钟的速率限制。
6. THE Desktop_App SHALL 在主界面提供已配对设备列表，并允许用户对任一设备执行"撤销授权"操作。
7. WHEN 用户对某设备执行"撤销授权"，THE Pairing_Service SHALL 立即吊销其 Session_Token 并断开对应的 Connection_Channel。
8. WHILE Web_Server 接收到来自非 LAN 子网（例如公网 IP）的请求，THE Web_Server SHALL 在进入业务逻辑前直接拒绝该请求并返回 403 状态码，且不执行配对、鉴权、消息分发等任何后续处理。
9. THE Pairing_Code SHALL 在 Desktop_App 重启后失效，由 Pairing_Service 重新生成。
10. WHERE 用户启用 HTTPS，THE Web_Server SHALL 仅通过 HTTPS 通道传输 Pairing_Code 与 Session_Token。

### Requirement 8: 多语言界面支持

**User Story:** 作为用户，我希望桌面应用与手机网页都能根据系统语言自动显示中文或英文，以便不同语言用户都能使用。

#### Acceptance Criteria

1. THE Desktop_App SHALL 至少支持简体中文（zh-CN）与英文（en-US）两种界面语言。
2. THE Mobile_Web_Client SHALL 至少支持简体中文（zh-CN）与英文（en-US）两种界面语言。
3. WHEN Desktop_App 首次启动，THE Desktop_App SHALL 根据操作系统当前区域设置自动选择默认界面语言。
4. WHEN Mobile_Web_Client 在浏览器中加载，THE Mobile_Web_Client SHALL 根据浏览器的 navigator.language 自动选择默认界面语言。
5. THE Desktop_App 与 Mobile_Web_Client SHALL 各自提供手动切换界面语言的入口，且切换后无需重启即可生效。
6. THE 界面语言选择 SHALL 与 ASR_Engine 的识别语言相互独立，互不影响。

### Requirement 9: 错误处理与连接稳定性

**User Story:** 作为用户，我希望连接出现波动或后台切换时应用能自动恢复，并在失败时给我清晰的提示，避免我反复手动刷新或重启。

#### Acceptance Criteria

1. WHILE Connection_Channel 已建立，THE Mobile_Web_Client 与 Desktop_App SHALL 每隔 20 秒互发心跳消息以检测连接活性。
2. IF 任意一端在 30 秒内未收到对端心跳，THEN 该端 SHALL 将连接标记为断开并触发重连流程。
3. WHEN Mobile_Web_Client 检测到 Connection_Channel 断开，THE Mobile_Web_Client SHALL 在 60 秒内最多发起 5 次自动重连，重连间隔遵循指数退避策略（1s、2s、4s、8s、16s）。
4. WHEN Mobile_Web_Client 在自动重连过程中识别到新的语音文本，THE Mobile_Web_Client SHALL 将该文本暂存到本地队列，并在重连成功后按时间顺序补发。
5. IF 自动重连达到最大次数仍未成功，THEN THE Mobile_Web_Client SHALL 显示"重连失败"提示并提供手动重试按钮。
6. WHEN Desktop_App 收到格式不合法或缺少 Session_Token 的消息，THE Desktop_App SHALL 丢弃该消息并通过 Connection_Channel 返回结构化错误对象（包含错误码、错误描述）。
7. THE Desktop_App SHALL 在本地日志中记录连接建立、断开、重连、注入失败等关键事件，单条日志大小不超过 4 KB，日志总量不超过 10 MB（达到上限后按时间滚动覆盖）。
8. IF Input_Injector 在执行注入时发生异常，THEN THE Desktop_App SHALL 记录错误日志并通过 Connection_Channel 通知 Mobile_Web_Client 该次注入失败及失败原因摘要。
9. WHEN Mobile_Web_Client 所在设备进入后台或屏幕锁定，THE Mobile_Web_Client SHALL 在重新进入前台时自动检查 Connection_Channel 状态并按需重连。
