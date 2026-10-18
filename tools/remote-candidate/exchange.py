"""Player-facing Exchange adapter; holds a launcher session, NEVER Authority service_token."""
import asyncio,base64,http.client,ipaddress,json,ssl,urllib.parse
from bridge import Rejected

class HttpExchange:
    def __init__(self,url,session,fixture_loopback=False,timeout=2):
        target=urllib.parse.urlsplit(url)
        if target.username or target.password or target.query or target.fragment or target.path not in ('','/'):
            raise Rejected('invalid exchange origin')
        if target.scheme!='https':
            if not fixture_loopback or target.scheme!='http':raise Rejected('HTTPS required')
            try:
                if not ipaddress.ip_address(target.hostname).is_loopback:raise Rejected('fixture requires loopback')
            except ValueError:raise Rejected('fixture requires numeric loopback') from None
        self.target=target;self.session=bytes(session);self.timeout=timeout
    async def __call__(self,challenge):
        return await asyncio.to_thread(self.request,challenge)
    def request(self,challenge):
        t=self.target
        conn=(http.client.HTTPSConnection(t.hostname,t.port,timeout=self.timeout,context=ssl.create_default_context())
              if t.scheme=='https' else http.client.HTTPConnection(t.hostname,t.port,timeout=self.timeout))
        try:
            # Neither a client user_id nor a native account identity is sent.
            body=json.dumps({'challenge':challenge},separators=(',',':')).encode()
            conn.request('POST','/v1/exchange',body,{'Content-Type':'application/json','X-Launcher-Session':base64.urlsafe_b64encode(self.session).decode().rstrip('=')})
            response=conn.getresponse();raw=response.read(16385)
            if response.status!=200 or len(raw)>16384:raise Rejected('exchange rejected')
            value=json.loads(raw)
            if not isinstance(value,dict) or set(value)!={'assertion'}:raise Rejected('invalid exchange response')
            return value['assertion']
        except Exception:
            raise Rejected('exchange unavailable or rejected') from None
        finally:conn.close()
