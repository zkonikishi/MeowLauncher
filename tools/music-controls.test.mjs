import assert from 'node:assert/strict';
import {mountMusic} from '../apps/launcher/ui/music.mjs';
class Element {checked=true;value='25';textContent='';attrs={};events={};addEventListener(n,f){this.events[n]=f;}setAttribute(n,v){this.attrs[n]=v;}removeEventListener(){}click(){this.events.dblclick?.({preventDefault(){}});}}
const elements=Object.fromEntries(['music-enabled','music-volume','music-state','music-toggle'].map(n=>[n,new Element()]));
const values=new Map();globalThis.localStorage={getItem:k=>values.get(k)??null,setItem:(k,v)=>values.set(k,v)};
globalThis.document={hidden:false,body:{append(){}},querySelector:()=>({hidden:true}),getElementById:k=>elements[k],addEventListener(){},removeEventListener(){}};
globalThis.window={addEventListener(){}};
let audio;globalThis.Audio=class {constructor(){audio=this;}pause(){this.paused=true;}async play(){this.paused=false;}};
const music=mountMusic();await Promise.resolve();
elements['music-toggle'].click();assert.equal(audio.paused,true);assert.equal(values.get('bgm.enabled'),'false');assert.equal(elements['music-toggle'].attrs['aria-pressed'],'false');
elements['music-toggle'].click();await Promise.resolve();assert.equal(audio.paused,false);assert.equal(elements['music-toggle'].attrs['aria-pressed'],'true');
elements['music-volume'].value='60';elements['music-volume'].events.input();assert.equal(audio.volume,.6);assert.equal(values.get('bgm.volume'),'60');
elements['music-volume'].value='0';elements['music-volume'].events.input();assert.equal(audio.paused,true);elements['music-toggle'].click();await Promise.resolve();assert.equal(audio.volume,.25);assert.equal(audio.paused,false);
music.stopForGame();assert.equal(audio.paused,true);
console.log('PASS: mute, unmute, persisted preference, button state, game-start pause');
