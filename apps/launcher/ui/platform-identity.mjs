export function validatePlatformCredentials(kind,identifier,password){
 const id=/^[A-Za-z0-9]{4,32}$/.test(identifier),uid=/^\d{8}$/.test(identifier)&&identifier!=='00000000',email=/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(identifier)&&identifier.length<=254;
 if(!({auto:id||uid||email,id,uid,email})[kind])return '请输入有效的登录 ID、UID 或邮箱。';
 if([...password].length<6||[...password].length>128||/[\u0000-\u001f\u007f]/.test(password))return '请输入 6–128 位密码。';
 return '';
}
export function validateRegistrationIdentity(id,nickname){
 if(!/^[A-Za-z0-9]{4,32}$/.test(id))return '登录 ID 须为 4–32 位英文字母或数字。';
 if(nickname&&(!nickname.trim()||[...nickname].length>64||/[\u0000-\u001f\u007f]/.test(nickname)))return '昵称须为 1–64 个字符。';
 return '';
}
