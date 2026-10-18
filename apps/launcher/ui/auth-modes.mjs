// Local UI names only. These are not wire values, issuer aliases or frozen API routes.
export const modeCopy=Object.freeze({
 account:Object.freeze({label:'自定义模式',identifierLabel:'8 位 UID',registerLabel:'注册账号',requiresPlatformIdentity:true}),
 simple:Object.freeze({label:'通用模式',identifierLabel:'用户名',registerLabel:'注册账号',requiresPlatformIdentity:false})
});
export function validateSimpleCredentials(username,password){
 // Exact DAO semantics: do not trim, fold case or normalize either field.
 if(typeof username!=='string'||! /^[A-Za-z0-9_-]{1,32}$/.test(username))return '用户名须为 1–32 位英文字母、数字、下划线或连字符。';
 if(typeof password!=='string')return '请输入密码。';
 const bytes=new TextEncoder().encode(password).byteLength;
 if(bytes<1||bytes>72)return '密码须为 1–72 个 UTF-8 字节，不能自动截断。';
 return null;
}
export function nativeAccountId(value){
 if(typeof value!=='string'||! /^[1-9][0-9]{0,18}$/.test(value)||BigInt(value)>9223372036854775807n)throw {code:'simple_invalid_response',message:'服务器返回的账号标识无效。'};
 return value;
}
// Not mounted in current app.js: host transport/admission schema has not frozen.
// `send` is an injected native-side implementation, never a browser DB connection.
// Its operation labels are local adapter calls, NOT invented HTTP/native routes.
export function simpleCredentialAdapter(send){
 return async function request(operation,username,password){
  if(!['register','authenticate'].includes(operation))throw {code:'simple_operation',message:'不支持的账号操作。'};
  const invalid=validateSimpleCredentials(username,password);
  if(invalid)throw {code:'simple_input',message:invalid};
  if(typeof send!=='function')throw {code:'simple_unavailable',message:'通用模式账号服务尚未接通。'};
  try{return await send(operation,{username,password});}
  catch{throw {code:'simple_rejected',message:operation==='register'?'注册未完成，请检查输入或稍后重试。':'登录未完成，请检查账号密码或稍后重试。'};}
 };
}
