const sharp=require('sharp'),path=require('node:path');
const qa=path.resolve(__dirname,'../docs/qa');
(async()=>{
 for(const [name,source] of [['modern','D:/Temp/QQ_1791878266419.png'],['modern-home','D:/Temp/codex-clipboard-b406dfb3-7b39-4ac4-b0f6-79b48fcdc5e5.png'],['retro',path.resolve(__dirname,'../docs/references/retro/resource-052.png')]]){
  const width=name==='retro'?687:1280,height=name==='retro'?599:728;
  await sharp({create:{width:width*2,height,channels:3,background:'#202020'}}).composite([
   {input:await sharp(source).resize(width,height).png().toBuffer(),left:0,top:0},
   {input:await sharp(path.join(qa,name+'.png')).resize(width,height).png().toBuffer(),left:width,top:0}
  ]).png().toFile(path.join(qa,name+'-comparison.png'));
 }
})();
