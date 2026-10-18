// Read-only extraction of supplied source assets, never executes source code.
const fs=require('node:fs'),path=require('node:path');
const out=path.resolve(__dirname,'../docs/references');fs.mkdirSync(out,{recursive:true});
const asar=fs.readFileSync('D:/Game/DNF/1.0.0.33/resources/app.asar');
const header=JSON.parse(asar.subarray(16,16+asar.readUInt32LE(12)).toString());
const base=8+asar.readUInt32LE(4), entries=[];
function walk(node,prefix=''){for(const [name,item] of Object.entries(node)){const rel=prefix+name;if(item.files)walk(item.files,rel+'/');else entries.push({path:rel,...item});}}
walk(header.files);fs.writeFileSync(path.join(out,'official-index.json'),JSON.stringify(entries.map(({path,size,unpacked})=>({path,size,unpacked})),null,2));
const official=path.join(out,'official');
let extracted=0;
for(const item of entries){
  if(item.unpacked||!/(renderer|assets|static|dist)/i.test(item.path)||! /\.(png|jpg|jpeg|webp|gif|svg|css|html|js)$/i.test(item.path))continue;
  if(item.path.includes('node_modules/'))continue;
  const dest=path.resolve(official,item.path);if(!dest.startsWith(official+path.sep))throw Error('Unsafe archive path');
  fs.mkdirSync(path.dirname(dest),{recursive:true});fs.writeFileSync(dest,asar.subarray(base+Number(item.offset),base+Number(item.offset)+item.size));extracted++;
}
const ePath='C:/workspace/Launcher/花枝登陆器/花枝6系源码/6系花枝网关源码开源项目/Source/登录器_复古登录器.e';
const data=fs.readFileSync(ePath),retro=path.join(out,'retro');fs.mkdirSync(retro,{recursive:true});
const records=[];
function save(start,end,extension){if(end<=start||end>data.length)return;const name=`resource-${String(records.length).padStart(3,'0')}.${extension}`;fs.writeFileSync(path.join(retro,name),data.subarray(start,end));records.push({name,offset:start,length:end-start});}
const png=Buffer.from([137,80,78,71,13,10,26,10]);
for(let pos=0;(pos=data.indexOf(png,pos))!==-1;pos+=8){let end=pos+8;while(end+12<data.length){const n=data.readUInt32BE(end),type=data.toString('ascii',end+4,end+8);if(n>data.length-end-12)break;end+=12+n;if(type==='IEND'){save(pos,end,'png');break;}}}
for(let pos=0;(pos=data.indexOf(Buffer.from([0xff,0xd8,0xff]),pos))!==-1;pos+=3){const end=data.indexOf(Buffer.from([0xff,0xd9]),pos+3);if(end!==-1)save(pos,end+2,'jpg');}
for(let pos=0;(pos=data.indexOf(Buffer.from('BM'),pos))!==-1;pos+=2){if(pos+54>data.length)continue;const size=data.readUInt32LE(pos+2),off=data.readUInt32LE(pos+10),dib=data.readUInt32LE(pos+14);if(size>54&&size<20e6&&off<size&&[40,108,124].includes(dib))save(pos,pos+size,'bmp');}
fs.writeFileSync(path.join(out,'retro-resources.json'),JSON.stringify(records,null,2));
console.log(JSON.stringify({officialEntries:entries.length,officialExtracted:extracted,retroResources:records.length}));
