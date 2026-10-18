export const defaults = () => ({version:1, clientDirectory:'', theme:'modern', rememberIdentifier:false, identifier:''});
export function validateCredentials(identifier, password) {
  const id=identifier.trim();
  if (id==='00000000') return '00000000 为保留 UID，请输入有效账号。';
  if (!/^\d{8}$/.test(id)) return '请输入 8 位 UID；忘记 UID 可先找回。';
  if (!password || [...password].length>128) return '请输入密码，最多 128 个字符。';
  return null;
}
export function settingsFromInputs(directory, theme, remember, identifier) {
  return {version:1, clientDirectory:directory.trim(), theme, rememberIdentifier:remember, identifier:remember&&/^\d{8}$/.test(identifier.trim())&&identifier.trim()!=='00000000'?identifier.trim():''};
}
export function errorMessage(error) {
  // The native layer only emits controlled errors. Unexpected JS/IPC errors
  // never surface raw request contents or credentials to the user or log.
  if (error && typeof error.code==='string' && typeof error.message==='string') return error.message;
  return '操作未完成，请打开诊断目录检查，或重新启动登陆器。';
}
