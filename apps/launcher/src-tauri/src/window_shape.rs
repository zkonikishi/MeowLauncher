//! Clip the native window as well as its WebView; CSS cannot clip an HWND.
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    #[repr(C)]
    struct Rect { left:i32, top:i32, right:i32, bottom:i32 }
    #[repr(C)]
    struct Point { x:i32, y:i32 }
    #[link(name="user32")]
    unsafe extern "system" {
        fn GetWindowRect(hwnd:*mut c_void, rect:*mut Rect)->i32;
        fn GetClientRect(hwnd:*mut c_void, rect:*mut Rect)->i32;
        fn ClientToScreen(hwnd:*mut c_void, point:*mut Point)->i32;
        fn GetDpiForWindow(hwnd:*mut c_void)->u32;
        fn SetWindowRgn(hwnd:*mut c_void, region:*mut c_void, redraw:i32)->i32;
        fn GetWindowRgn(hwnd:*mut c_void, region:*mut c_void)->i32;
    }
    #[link(name="gdi32")]
    unsafe extern "system" {
        fn CreateRoundRectRgn(left:i32,top:i32,right:i32,bottom:i32,width:i32,height:i32)->*mut c_void;
        fn DeleteObject(object:*mut c_void)->i32;
        fn EqualRgn(first:*mut c_void,second:*mut c_void)->i32;
        #[cfg(test)] fn PtInRegion(region:*mut c_void,x:i32,y:i32)->i32;
    }
    #[link(name="dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd:*mut c_void,attribute:u32,value:*const c_void,size:u32)->i32;
    }
    pub fn apply(hwnd:*mut c_void) {
        unsafe {
            let rounded=2u32;
            let no_border=0xfffffffeu32;
            DwmSetWindowAttribute(hwnd,33,&rounded as *const _ as _,4);
            DwmSetWindowAttribute(hwnd,34,&no_border as *const _ as _,4);
            let mut rect=Rect{left:0,top:0,right:0,bottom:0};
            if GetWindowRect(hwnd,&mut rect)==0 {return;}
            // Frameless windows may retain non-client resize margins. Clipping
            // the outer HWND rounds those white margins instead of the image.
            // Use the WebView's client bounds, translated into window pixels.
            let mut client=Rect{left:0,top:0,right:0,bottom:0};
            let mut origin=Point{x:0,y:0};
            if GetClientRect(hwnd,&mut client)==0 || ClientToScreen(hwnd,&mut origin)==0 {return;}
            let left=origin.x-rect.left;
            let top=origin.y-rect.top;
            let right=left+client.right-client.left;
            let bottom=top+client.bottom-client.top;
            if right<=left || bottom<=top {return;}
            let diameter=(40u32.saturating_mul(GetDpiForWindow(hwnd).max(96))/96) as i32;
            let region=CreateRoundRectRgn(left,top,right,bottom,diameter,diameter);
            let current=CreateRoundRectRgn(0,0,1,1,0,0);
            if !current.is_null(){
                let unchanged=GetWindowRgn(hwnd,current)!=0&&!region.is_null()&&EqualRgn(current,region)!=0;
                DeleteObject(current);
                if unchanged{DeleteObject(region);return;}
            }
            if !region.is_null() && SetWindowRgn(hwnd,region,1)==0 {DeleteObject(region);}
            // A successful SetWindowRgn transfers ownership to Windows.
        }
    }
    #[test]
    fn excludes_non_client_border_at_scaled_dpi() {
        unsafe {
            // A 150% window with 12px side/bottom frame margins. The page
            // begins at x=12; none of the frame may survive the region.
            let region=CreateRoundRectRgn(12,0,1933,1093,60,60);
            assert!(!region.is_null());
            assert_eq!(PtInRegion(region,0,500),0);
            assert_eq!(PtInRegion(region,11,500),0);
            assert_eq!(PtInRegion(region,1940,500),0);
            assert_eq!(PtInRegion(region,960,1100),0);
            assert_eq!(PtInRegion(region,12,0),0);
            assert_ne!(PtInRegion(region,12,500),0);
            assert_ne!(PtInRegion(region,960,0),0);
            DeleteObject(region);
        }
    }
    #[test]
    fn clips_native_corners_without_clipping_center() {
        unsafe {
            let region=CreateRoundRectRgn(0,0,1281,729,40,40);
            assert!(!region.is_null());
            assert_eq!(PtInRegion(region,0,0),0);
            assert_eq!(PtInRegion(region,1279,0),0);
            assert_eq!(PtInRegion(region,0,727),0);
            assert_eq!(PtInRegion(region,1279,727),0);
            assert_ne!(PtInRegion(region,640,364),0);
            assert_ne!(PtInRegion(region,640,0),0);
            DeleteObject(region);
        }
    }
}
pub fn apply<R:tauri::Runtime>(window:&tauri::Window<R>) {
    #[cfg(windows)]
    if let Ok(hwnd)=window.hwnd(){win::apply(hwnd.0 as _);}
}
pub fn apply_webview<R:tauri::Runtime>(window:&tauri::WebviewWindow<R>) {
    #[cfg(windows)]
    if let Ok(hwnd)=window.hwnd(){win::apply(hwnd.0 as _);}
}
