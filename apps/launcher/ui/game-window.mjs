import {errorMessage} from './model.mjs';
export async function mountGameWindow(invoke){
 const section=document.getElementById('game-window-settings'),select=document.getElementById('game-window-size'),button=document.getElementById('save-game-window'),state=document.getElementById('game-window-state');
 section.hidden=false;
 try{const preference=await invoke('game_window_preferences');if(preference?.size)select.value=`${preference.size.width}x${preference.size.height}`;}catch(error){state.textContent=errorMessage(error);}
 button.onclick=async()=>{
  button.disabled=true;
  try{const [width,height]=select.value.split('x').map(Number);const preference={size:select.value==='keep'?null:{width,height}};await invoke('save_game_window_preferences',{preference});state.textContent='窗口尺寸已保存，下次启动游戏时应用。游戏正在运行时不会修改配置。';}
  catch(error){state.textContent=errorMessage(error);}finally{button.disabled=false;}
 };
}
