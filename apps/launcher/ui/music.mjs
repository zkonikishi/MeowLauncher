import {bindClickActions} from './click-actions.mjs';
export function mountMusic(){
 const audio=new Audio(new URL('./assets/wind-warrior.ogg',import.meta.url).href);
 audio.id='launcher-bgm';audio.loop=true;audio.preload='metadata';document.body.append(audio);
 const enabled=document.getElementById('music-enabled'),volume=document.getElementById('music-volume'),state=document.getElementById('music-state');
 const toggle=document.getElementById('music-toggle');
 function renderToggle(){const percent=document.getElementById('music-percent');if(percent)percent.textContent=volume.value+'%';if(toggle){toggle.setAttribute('aria-pressed',String(enabled.checked&&Number(volume.value)>0));const icon=toggle.querySelector?.('img');if(icon)icon.src=enabled.checked&&Number(volume.value)>0?'assets/icons/volume-2.svg':'assets/icons/volume-x.svg';toggle.title='单击调节音量 · 双击'+(enabled.checked&&Number(volume.value)>0?'关闭音乐':'开启音乐');}}
 let stoppedForGame=false,disposed=false;
 try{enabled.checked=localStorage.getItem('bgm.enabled')!=='false';const v=localStorage.getItem('bgm.volume');if(v!==null&&Number.isFinite(Number(v)))volume.value=String(Math.max(0,Math.min(100,Number(v))));}catch{}
 audio.volume=Number(volume.value)/100;
 function save(){try{localStorage.setItem('bgm.enabled',String(enabled.checked));localStorage.setItem('bgm.volume',volume.value);}catch{}}
 async function play(){
  if(disposed||!enabled.checked||document.hidden||stoppedForGame)return;
  try{await audio.play();if(disposed||!enabled.checked||document.hidden||stoppedForGame)audio.pause();else state.textContent='正在循环播放';}
  catch(e){state.textContent=e.name==='NotAllowedError'?'点击界面后开始播放':'背景音乐暂时无法播放';}
 }
 function sync(){renderToggle();if(!enabled.checked||document.hidden||stoppedForGame){audio.pause();state.textContent=stoppedForGame?'游戏已启动，音乐已停止':enabled.checked?'已暂停':'已关闭';}else void play();}
 const popover=document.querySelector('.music-popover');
 function closeVolume(){popover.hidden=true;toggle.setAttribute('aria-expanded','false');}
 popover.hidden=true;toggle.setAttribute('aria-expanded','false');
 const unbind=bindClickActions(toggle,()=>{popover.hidden=!popover.hidden;toggle.setAttribute('aria-expanded',String(!popover.hidden));},()=>{closeVolume();enabled.checked=!enabled.checked;if(enabled.checked&&Number(volume.value)===0){volume.value='25';audio.volume=.25;}stoppedForGame=false;save();sync();});
 const outside=e=>{if(!e.target.closest('.music-control'))closeVolume();};
 const escape=e=>{if(e.key==='Escape')closeVolume();};
 document.addEventListener('pointerdown',outside);document.addEventListener('keydown',escape);
 enabled.addEventListener('change',()=>{stoppedForGame=false;save();sync();});
 volume.addEventListener('input',()=>{audio.volume=Number(volume.value)/100;enabled.checked=Number(volume.value)>0;save();sync();});
 const gesture=()=>{if(audio.paused)void play();};
 document.addEventListener('pointerdown',gesture);document.addEventListener('keydown',gesture);document.addEventListener('visibilitychange',sync);
 window.addEventListener('pagehide',()=>{disposed=true;unbind();document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',escape);audio.pause();document.removeEventListener('pointerdown',gesture);document.removeEventListener('keydown',gesture);document.removeEventListener('visibilitychange',sync);},{once:true});
 renderToggle();void play();
 return {stopForGame(){stoppedForGame=true;sync();},resumeAfterGame(){stoppedForGame=false;sync();}};
}
