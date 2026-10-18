// Extract only image streams from the supplied launcher's resource cache.
const fs=require('node:fs'),path=require('node:path'),sharp=require('sharp');
const out=path.resolve(__dirname,'../docs/references');
async function main(){
 const dir=path.join(out,'official-art');fs.mkdirSync(dir,{recursive:true});
 let n=0;
 for(const entry of fs.readdirSync('D:/Game/DNF/AppData/Cache/Cache_Data',{withFileTypes:true})){
  if(!entry.isFile())continue;
  const b=fs.readFileSync('D:/Game/DNF/AppData/Cache/Cache_Data/'+entry.name);
  for(const [sig,ext] of [[Buffer.from([137,80,78,71,13,10,26,10]),'png'],[Buffer.from([255,216,255]),'jpg'],[Buffer.from('RIFF'),'webp']]){
   let p=-1;
   while((p=b.indexOf(sig,p+1))>=0){
    if(ext==='webp'&&b.toString('ascii',p+8,p+12)!=='WEBP')continue;
    try{const raw=b.subarray(p),m=await sharp(raw).metadata();if(m.width<500||m.height<250||m.width/m.height<1.4)continue;
     await sharp(raw).png().toFile(path.join(dir,`art-${n++}.png`));
    }catch{}
   }
  }
 }
 for(const [name,folder] of [['official',dir],['retro',path.join(out,'retro')]]){
  const rows=[];
  for(const f of fs.readdirSync(folder)){try{const m=await sharp(path.join(folder,f)).metadata();if(m.width>=500&&m.height>=250)rows.push({file:f,width:m.width,height:m.height});}catch{}}
  fs.writeFileSync(path.join(out,name+'-art.json'),JSON.stringify(rows,null,2));
  for(let offset=0;offset<rows.length;offset+=24){
   const batch=rows.slice(offset,offset+24),parts=[];
   for(let i=0;i<batch.length;i++){const r=batch[i],x=i%4*260,y=Math.floor(i/4)*180;
    parts.push({input:await sharp(path.join(folder,r.file)).resize(256,148,{fit:'contain',background:'#252525'}).png().toBuffer(),left:x,top:y});
    parts.push({input:Buffer.from(`<svg width="260" height="30"><text x="3" y="20" fill="white" font-size="13">${r.file} ${r.width}×${r.height}</text></svg>`),left:x,top:y+148});
   }
   await sharp({create:{width:1040,height:Math.ceil(batch.length/4)*180,channels:3,background:'#252525'}}).composite(parts).png().toFile(path.join(out,`${name}-sheet-${offset/24}.png`));
  }
  console.log(name,rows.length);
 }
}
main().catch(e=>{console.error(e.message);process.exitCode=1;});
