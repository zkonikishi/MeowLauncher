// Defer pointer single-click actions so a double click never also changes an image.
export function bindClickActions(element,single,double){
 let pending=null;
 const cancel=()=>{clearTimeout(pending);pending=null;};
 const click=event=>{cancel();if(event.detail===0){single();return;}if(event.detail>1)return;pending=setTimeout(()=>{pending=null;single();},500);};
 const dblclick=event=>{event.preventDefault();cancel();double();};
 element.addEventListener('click',click);element.addEventListener('dblclick',dblclick);
 return ()=>{cancel();element.removeEventListener('click',click);element.removeEventListener('dblclick',dblclick);};
}
