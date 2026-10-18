"""Import a backend-generated public target into the locally pinned review template.
Not a production signed config parser; no session/password/service secret fields allowed.
"""
import argparse,json,os,tempfile
from pathlib import Path
from manual import check_config
from bridge import Rejected

TARGET_KEYS={'profile','review_only','auth_mode','deployment','upstream','exchange_origin','bind_port'}
def load_json(path):
 def unique(pairs):
  out={}
  for key,value in pairs:
   if key in out:raise Rejected('duplicate profile key')
   out[key]=value
  return out
 p=Path(path)
 if p.stat().st_size>16384:raise Rejected('profile too large')
 return json.loads(p.read_text(encoding='utf-8-sig'),object_pairs_hook=unique)

def import_target(target_path,template_path,destination):
 target=load_json(target_path);template=load_json(template_path);destination=Path(destination).resolve()
 if not isinstance(target,dict) or set(target) not in (TARGET_KEYS,TARGET_KEYS|{'channels'}) or target['profile']!='account-review-target-v1' or target['review_only'] is not True or target['auth_mode']!='account':raise Rejected('invalid public target profile')
 if destination.drive.upper()=='C:' or destination.exists():raise Rejected('destination must be new and outside C drive')
 for key in ('review_only','auth_mode','deployment','upstream','exchange_origin','bind_port'):template[key]=target[key]
 if 'channels' in target:template['channels']=target['channels']
 destination.parent.mkdir(parents=True,exist_ok=True)
 staged=None
 try:
  with tempfile.NamedTemporaryFile('w',encoding='utf-8',dir=destination.parent,prefix='.review-import-',suffix='.json',delete=False) as file:
   staged=Path(file.name);json.dump(template,file,ensure_ascii=False,indent=2)
  check_config(staged) # real local file pins + URL/scope/port validation, no network/start
  # Exclusive-create avoids accidentally replacing another owner's configuration.
  with destination.open('x',encoding='utf-8') as output:output.write(staged.read_text(encoding='utf-8'))
 finally:
  if staged:staged.unlink(missing_ok=True)
 return destination

def main():
 parser=argparse.ArgumentParser(description='导入隔离环境生成的公开目标配置；不接收任何私有凭据')
 parser.add_argument('--target',required=True);parser.add_argument('--template',required=True);parser.add_argument('--output',required=True)
 parser.add_argument('--review-unsigned-config',action='store_true');args=parser.parse_args()
 if not args.review_unsigned_config:raise Rejected('review-only config trust is not integrated')
 import_target(args.target,args.template,args.output)
 print('目标配置已导入并通过本地文件摘要校验；未联网、未启动程序。')
if __name__=='__main__':
 try:main()
 except Exception:print('导入未完成：检查目标配置、文件摘要与新输出路径；不导入密码或服务密钥。');raise SystemExit(1)
