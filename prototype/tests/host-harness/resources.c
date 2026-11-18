// SPDX-License-Identifier: GPL-2.0
// Actual grant.c/pipe.c/resource ledger with mocked Linux allocation primitives.
#include "shim.h"
#include "../../linux/endovisor/typedio_resources.h"
static struct klet_tio_call operation(struct klet_tio_endpoint e,u32 op,u32 role,u32 len,u64 token){struct klet_tio_call c={.operation=op,.arg0=e.slot,.arg1=e.generation};c.record.status=role;c.record.len=len;c.record.generation=token;return c;}
int main(void){
 assert(!klet_tio_resource_owner_register(10));assert(!klet_tio_resource_owner_register(20));assert(!klet_tio_resource_owner_register(30));
 struct klet_tio_pipe_account *account=klet_tio_pipe_account_create(10,1<<20,32);assert(!IS_ERR(account));
 struct klet_tio_pipe *control;struct klet_tio_endpoint endpoints[2];assert(!klet_tio_role_pair(account,10,16,&control,endpoints));
 struct klet_tio_transfer tx={0};assert(!klet_tio_transfer_prepare(&tx,endpoints[0],10,20,true));
 struct klet_tio_endpoint invalid=endpoints[1];invalid.generation++;
 assert(klet_tio_transfer_prepare(&tx,invalid,10,30,false)==-ESTALE);klet_tio_transfer_rollback(&tx);
 struct klet_tio_call r=operation(endpoints[0],16,0,16,0);assert(!klet_tio_resource_service(10,&r));u64 token=r.record.generation;
 assert(klet_tio_transfer_prepare(&tx,endpoints[0],10,20,true)==-EBUSY);
 r.operation=19;r.record.generation=token;assert(!klet_tio_resource_service(10,&r));
 assert(!klet_tio_transfer_prepare(&tx,endpoints[0],10,20,true));assert(!klet_tio_transfer_prepare(&tx,endpoints[1],10,30,false));assert(!klet_tio_transfer_commit(&tx));
 klet_tio_endpoint_drop(endpoints[0],10);r=operation(endpoints[0],16,0,16,0);assert(klet_tio_resource_service(10,&r)==-ESTALE);
 assert(!klet_tio_resource_service(20,&r));token=r.record.generation;assert(r.record.len==16);memcpy((void *)(uintptr_t)r.record.sequence,"abcdef",6);
 r.operation=17;r.record.len=6;assert(!klet_tio_resource_service(20,&r));r.operation=18;assert(!klet_tio_resource_service(20,&r));
 struct klet_tio_call read=operation(endpoints[1],16,1,16,0);assert(!klet_tio_resource_service(30,&read));assert(read.record.len==6);
 char buffer[6]={0};int fault=-1;assert(klet_tio_pipe_copy_user(control,30,1,read.record.generation,buffer,6,&fault)==6);assert(!fault&&!memcmp(buffer,"abcdef",6));
 read.operation=18;read.record.len=0;assert(!klet_tio_resource_service(30,&read)); // peek
 read=operation(endpoints[1],16,1,16,0);assert(!klet_tio_resource_service(30,&read));assert(read.record.len==6);read.operation=19;assert(!klet_tio_resource_service(30,&read));
 klet_tio_endpoint_drop(endpoints[0],20);read=operation(endpoints[1],16,1,16,0);assert(!klet_tio_resource_service(30,&read));read.operation=17;read.record.len=6;assert(!klet_tio_resource_service(30,&read));read.operation=18;assert(!klet_tio_resource_service(30,&read));read=operation(endpoints[1],16,1,16,0);assert(klet_tio_resource_service(30,&read)==1);
 klet_tio_endpoint_drop(endpoints[1],30);klet_tio_pipe_put(control);
 // Both event destinations are reserved transactionally, including a quota
 // failure at the second destination after the first was provisionally held.
 assert(!klet_tio_resource_owner_register(40));
 struct klet_tio_pipe *filled[16];struct klet_tio_endpoint fill_keys[16][2];
 for(unsigned int i=0;i<16;i++){
  assert(!klet_tio_role_pair(account,10,16,&filled[i],fill_keys[i]));
  for(unsigned int j=0;j<2;j++)assert(!klet_tio_transfer_prepare(&tx,fill_keys[i][j],10,40,true));
  assert(!klet_tio_transfer_commit(&tx));
 }
 struct klet_tio_pipe *preparing[2];struct klet_tio_endpoint prepare_keys[2][2];struct klet_tio_resource items[4];u64 targets[4]={30,30,40,40};
 for(unsigned int i=0;i<2;i++){assert(!klet_tio_role_pair(account,10,16,&preparing[i],prepare_keys[i]));for(unsigned int j=0;j<2;j++)items[2*i+j]=(struct klet_tio_resource){prepare_keys[i][j].slot,prepare_keys[i][j].generation,10,j+1,0};}
 assert(klet_tio_deliver_roles(items,targets,4)==-EDQUOT);
 for(unsigned int i=0;i<2;i++){for(unsigned int j=0;j<2;j++)klet_tio_endpoint_drop(prepare_keys[i][j],10);klet_tio_pipe_put(preparing[i]);}
 for(unsigned int i=0;i<16;i++){for(unsigned int j=0;j<2;j++)klet_tio_endpoint_drop(fill_keys[i][j],40);klet_tio_pipe_put(filled[i]);}
 klet_tio_resource_owner_release(40);
 // Forgotten guest reservations are canceled only after carrier eviction.
 assert(!klet_tio_resource_owner_register(50));assert(!klet_tio_resource_owner_register(60));
 assert(!klet_tio_role_pair(account,10,16,&control,endpoints));
 assert(!klet_tio_transfer_prepare(&tx,endpoints[0],10,50,true));assert(!klet_tio_transfer_prepare(&tx,endpoints[1],10,60,true));assert(!klet_tio_transfer_commit(&tx));
 r=operation(endpoints[0],16,0,16,0);assert(!klet_tio_resource_service(50,&r));
 klet_tio_resource_owner_release(50);klet_tio_resource_evicted(50);
 r=operation(endpoints[0],16,0,16,0);assert(klet_tio_resource_service(50,&r)==-ESTALE);
 read=operation(endpoints[1],16,1,16,0);assert(klet_tio_resource_service(60,&read)==1);
 klet_tio_endpoint_drop(endpoints[1],60);klet_tio_resource_owner_release(60);klet_tio_pipe_put(control);
 // Host-origin reservations survive even an erroneous eviction request. A
 // host faulting copy must end on its own; there is no forced host GC.
 assert(!klet_tio_resource_owner_register(70));klet_tio_resource_owner_host(70);assert(!klet_tio_resource_owner_register(80));
 assert(!klet_tio_role_pair(account,10,16,&control,endpoints));
 assert(!klet_tio_transfer_prepare(&tx,endpoints[0],10,70,true));assert(!klet_tio_transfer_prepare(&tx,endpoints[1],10,80,true));assert(!klet_tio_transfer_commit(&tx));
 r=operation(endpoints[0],16,0,16,0);assert(!klet_tio_resource_service(70,&r));
 klet_tio_resource_owner_release(70);klet_tio_resource_evicted(70);
 r.operation=19;assert(!klet_tio_resource_service(70,&r));
 klet_tio_endpoint_drop(endpoints[1],80);klet_tio_resource_owner_release(80);klet_tio_pipe_put(control);
 // Target death between prepare and commit must preserve source ownership.
 assert(!klet_tio_role_pair(account,10,16,&control,endpoints));assert(!klet_tio_transfer_prepare(&tx,endpoints[0],10,20,true));klet_tio_resource_revoke(20);assert(klet_tio_transfer_commit(&tx)==-ESTALE);klet_tio_transfer_rollback(&tx);
 klet_tio_endpoint_drop(endpoints[0],10);klet_tio_endpoint_drop(endpoints[1],10);klet_tio_pipe_put(control);
 klet_tio_resource_owner_release(10);klet_tio_resource_owner_release(20);klet_tio_resource_owner_release(30);klet_tio_pipe_account_put(account);
 // Real grant backing survives dropping the instance owner while a host lease exists.
 struct klet_inst inst={.lock=PTHREAD_MUTEX_INITIALIZER};inst.grant_backing=klet_grant_alloc(1);assert(inst.grant_backing);
 struct page *page=klet_grant_pages(inst.grant_backing);inst.boot.grant_paddr=page_to_phys(page);inst.boot.grant_len=2*PAGE_SIZE;
 unsigned long flags;spin_lock_irqsave(&inst.lock,flags);struct klet_grant_backing *lease=klet_grant_lease_get(&inst,inst.boot.grant_paddr,16);assert(lease);assert(!klet_grant_lease_get(&inst,UINT64_MAX-1,16));spin_unlock_irqrestore(&inst.lock,flags);
 ((char *)page_address(page))[0]=42;klet_grant_put(inst.grant_backing);inst.grant_backing=NULL;assert(((char *)page_address(page))[0]==42);klet_grant_lease_put(lease);
 assert(!atomic_load(&native_module.references));puts("PASS native mock: real pipe/transfer rollback/ownership/peek/EOF/target death/guest eviction/host reservation quarantine/grant lease");return 0;
}
