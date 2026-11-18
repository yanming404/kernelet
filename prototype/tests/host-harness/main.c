// SPDX-License-Identifier: GPL-2.0
// Actual host.c and Rust QueueCore, mocked Linux primitives only.
#include "shim.h"
#include "../../linux/endovisor/typedio_host.h"
struct context {pthread_mutex_t lock;pthread_cond_t changed;struct klet_tio_request *pending[8];unsigned int arrivals;};
static struct context *fixture;
static atomic_int released;
static atomic_int removed_count;
static int prepare(const struct klet_tio_descriptor *d,u64 generation,void **result){(void)generation;if(d->config_len!=1)return -EINVAL;fixture=calloc(1,sizeof(*fixture));assert(fixture);pthread_mutex_init(&fixture->lock,NULL);pthread_cond_init(&fixture->changed,NULL);*result=fixture;return 0;}
static void handle(void *opaque,struct klet_tio_request *request){struct context *c=opaque;pthread_mutex_lock(&c->lock);for(int i=0;i<8;i++)if(!c->pending[i]){c->pending[i]=request;c->arrivals++;pthread_cond_broadcast(&c->changed);pthread_mutex_unlock(&c->lock);return;}abort();}
static void removed(void *opaque){struct context *c=opaque;pthread_mutex_lock(&c->lock);for(int i=0;i<8;i++)if(c->pending[i]){struct klet_tio_record error={.status=2};struct klet_tio_request *r=c->pending[i];c->pending[i]=NULL;klet_tio_request_finish(r,&error);}pthread_cond_broadcast(&c->changed);pthread_mutex_unlock(&c->lock);atomic_fetch_add(&removed_count,1);}
static void release_context(void *opaque){struct context *c=opaque;pthread_cond_destroy(&c->changed);pthread_mutex_destroy(&c->lock);free(c);atomic_fetch_add(&released,1);}
static void wait_arrivals(unsigned int count){struct timespec limit;clock_gettime(CLOCK_REALTIME,&limit);limit.tv_sec+=5;pthread_mutex_lock(&fixture->lock);while(fixture->arrivals<count)assert(!pthread_cond_timedwait(&fixture->changed,&fixture->lock,&limit));pthread_mutex_unlock(&fixture->lock);}
static struct klet_tio_request *take_request(u64 tag){struct klet_tio_request *r=NULL;pthread_mutex_lock(&fixture->lock);for(int i=0;i<8;i++)if(fixture->pending[i]&&klet_tio_request_message(fixture->pending[i])->tag==tag){r=fixture->pending[i];fixture->pending[i]=NULL;break;}pthread_mutex_unlock(&fixture->lock);assert(r);return r;}
static void finish(struct klet_tio_request *r,unsigned char value,u32 status){struct klet_tio_record reply={.len=1,.status=status};reply.data[0]=value;klet_tio_request_finish(r,&reply);}
static struct klet_tio_call qc(struct klet_tio_call opened,u32 operation){struct klet_tio_call c={.operation=operation,.handle=opened.handle,.generation=opened.generation,.arg0=opened.arg0,.arg1=opened.arg1};return c;}
static struct klet_tio_call submit(struct klet_carrier *carrier,struct klet_tio_call opened,u64 tag,unsigned char value){struct klet_tio_call c=qc(opened,KLET_TIO_SUBMIT);c.record.tag=tag;c.record.len=1;c.record.data[0]=value;assert(!klet_tio_service(carrier,&c));return c;}
int main(void){
 struct cgroup owner={1},other={2};native_task.group=&owner;
 struct klet_inst inst={.mutex=PTHREAD_MUTEX_INITIALIZER,.lock=PTHREAD_MUTEX_INITIALIZER,.nr_vcpus=1,.cgrp=&owner};refcount_set(&inst.refs,1);
 struct klet_carrier carrier={.inst=&inst};inst.vcpus[0]=&carrier;
 struct klet_tio_backend backend={.module=THIS_MODULE,.prepare=prepare,.handle=handle,.remove=removed,.release=release_context};
 backend.descriptor.abi_version=KLET_TIO_ABI;backend.descriptor.protocol_version=1;backend.descriptor.protocol[0]=42;for(int i=0;i<3;i++)backend.descriptor.schemas[i][0]=i+1;
 assert(!klet_tio_host_init());assert(!klet_tio_backend_register(&backend));
 struct klet_tio_descriptor bad=backend.descriptor;bad.schemas[1][0]=9;assert(klet_tio_create_device(&bad)==-ENOENT);
 struct klet_tio_descriptor profile=backend.descriptor;profile.config_len=1;profile.config[0]=7;
 int fd=klet_tio_create_device(&profile);assert(fd>=3);struct klet_device attach={.type=KLET_DEVICE_TYPEDIO,.fd=fd};
 native_task.group=&other;assert(klet_tio_attach(&inst,&attach)==-EPERM);native_task.group=&owner;assert(klet_tio_attach(&inst,&attach)==0);assert(klet_tio_attach(&inst,&attach)==-EBUSY);
 struct klet_tio_call discovered={.operation=KLET_TIO_DISCOVER};assert(!klet_tio_service(&carrier,&discovered));assert(!memcmp(&discovered.descriptor,&profile,sizeof(profile)));assert(discovered.generation);
 struct klet_tio_call opened={.operation=KLET_TIO_OPEN,.generation=discovered.generation,.arg0=2,.descriptor=discovered.descriptor};opened.descriptor.schemas[2][0]=0;assert(klet_tio_service(&carrier,&opened)==-EPROTO);opened.descriptor=discovered.descriptor;assert(!klet_tio_service(&carrier,&opened));
 struct klet_tio_call a=submit(&carrier,opened,11,0x11);submit(&carrier,opened,12,0x22);
 struct klet_tio_call full=qc(opened,KLET_TIO_SUBMIT);full.record.tag=13;assert(klet_tio_service(&carrier,&full)==-EAGAIN);full.record.tag=11;assert(klet_tio_service(&carrier,&full)==-EEXIST);
 struct klet_tio_call unauthorized_finish=qc(opened,101);unauthorized_finish.record=a.record;assert(klet_tio_service(&carrier,&unauthorized_finish)==-EINVAL);
 wait_arrivals(2);struct klet_tio_request *first=take_request(11),*second=take_request(12);assert(klet_tio_request_message(first)->data[0]==0x11);assert(klet_tio_request_caller(first)==discovered.generation);
 struct klet_tio_call cancel=qc(opened,KLET_TIO_CANCEL);cancel.record=a.record;assert(!klet_tio_service(&carrier,&cancel));assert(klet_tio_request_cancelled(first));assert(!klet_tio_request_begin_access(first));finish(first,0,1);assert(klet_tio_request_begin_access(first)==-EBUSY);finish(second,0x22,0);
 struct klet_tio_call pop=qc(opened,KLET_TIO_POP);assert(!klet_tio_service(&carrier,&pop));assert(pop.record.tag==12&&pop.record.data[0]==0x22);assert(klet_tio_service(&carrier,&pop)==1);klet_tio_request_end_access(first);assert(!klet_tio_service(&carrier,&pop));assert(pop.record.tag==11&&pop.record.status==1);
 struct klet_tio_call bind=qc(opened,KLET_TIO_BIND);bind.record.generation=55;bind.record.sequence=7;assert(!klet_tio_service(&carrier,&bind));struct klet_tio_call events=qc(opened,KLET_TIO_EVENTS);klet_tio_service(&carrier,&events);struct klet_tio_call rearm=qc(opened,KLET_TIO_REARM);assert(klet_tio_service(&carrier,&rearm)==1);
 submit(&carrier,opened,13,0x33);wait_arrivals(3);finish(take_request(13),0x33,0);assert(atomic_exchange(&carrier.pending,0)&KLET_VIRQ_LINES);
 struct klet_tio_call irq={.operation=KLET_TIO_IRQ_POLL};assert(!klet_tio_service(&carrier,&irq));assert(irq.record.sequence==7&&irq.record.generation==55);assert(klet_tio_service(&carrier,&irq)==1);assert(klet_tio_service(&carrier,&events)&1);assert(!klet_tio_service(&carrier,&pop));klet_tio_service(&carrier,&events);assert(klet_tio_service(&carrier,&rearm)==1);
 struct klet_tio_call close=qc(opened,KLET_TIO_CLOSE);assert(!klet_tio_service(&carrier,&close));assert(klet_tio_service(&carrier,&pop)==-ESTALE);
 struct klet_tio_call replacement={.operation=KLET_TIO_OPEN,.generation=discovered.generation,.arg0=2,.descriptor=discovered.descriptor};assert(!klet_tio_service(&carrier,&replacement));assert(replacement.arg1!=opened.arg1);assert(klet_tio_service(&carrier,&cancel)==-ESTALE);
 submit(&carrier,replacement,44,0x44);wait_arrivals(4);
 struct klet_tio_request *active=NULL;pthread_mutex_lock(&fixture->lock);for(int i=0;i<8;i++)if(fixture->pending[i]&&klet_tio_request_message(fixture->pending[i])->tag==44)active=fixture->pending[i];pthread_mutex_unlock(&fixture->lock);assert(active);assert(!klet_tio_request_begin_access(active));
 struct file *provider=fget(fd);assert(provider);assert(!provider->f_op->unlocked_ioctl(provider,KLET_TIO_REMOVE_DEVICE,0));fput(provider);
 for(int i=0;i<10000&&!atomic_load(&removed_count);i++)usleep(100);assert(atomic_load(&removed_count));
 struct klet_tio_call removed_events=qc(replacement,KLET_TIO_EVENTS);assert(klet_tio_service(&carrier,&removed_events)&4);
 struct klet_tio_call removed_submit=qc(replacement,KLET_TIO_SUBMIT);removed_submit.record.tag=99;assert(klet_tio_service(&carrier,&removed_submit)==-ENODEV);
 struct klet_tio_call removed_pop=qc(replacement,KLET_TIO_POP);assert(klet_tio_service(&carrier,&removed_pop)==1);klet_tio_request_end_access(active);assert(!klet_tio_service(&carrier,&removed_pop));assert(removed_pop.record.status==2);native_close(fd);
 inst.dying=true;klet_tio_detach_instance(&inst);klet_tio_host_exit();
 assert(atomic_load(&released)==1);assert(atomic_load(&native_module.references)==0);assert(atomic_load(&inst.refs.value)==1);assert(!klet_tio_backend_unregister(&backend));
 puts("PASS native mock: attachment/discovery/open/dispatch/completion/cancel/retention/IRQ/rearm/generation/death");return 0;
}
