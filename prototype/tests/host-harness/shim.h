// SPDX-License-Identifier: GPL-2.0
// Native mock of Linux primitives only. Production host/queue code is unchanged.
#ifndef TIO_NATIVE_SHIM_H
#define TIO_NATIVE_SHIM_H
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/epoll.h>
#include "../../linux/endovisor/uapi/typedio.h"
typedef uint64_t u64;
typedef int64_t s64;
typedef uint32_t u32;
#define __user
#define GFP_KERNEL_ACCOUNT 1
#define __GFP_ZERO 2
#define __GFP_NOWARN 4
#define PAGE_SIZE 4096UL
#define MAX_PAGE_ORDER 10
#define U64_MAX UINT64_MAX
#define O_CLOEXEC_NATIVE 0
#define KLET_DEVICE_BLOCK 1
#define KLET_DEVICE_NET 2
#define KLET_DEVICE_TYPEDIO 3
#define KLET_VIRQ_LINES 4
#define WQ_UNBOUND 0
#define WQ_MEM_RECLAIM 0
#define S64_MAX INT64_MAX
#define U32_MAX UINT32_MAX
#define THIS_MODULE (&native_module)
#define READ_ONCE(value) __atomic_load_n(&(value),__ATOMIC_ACQUIRE)
#define WARN_ON(value) ((value)?(fprintf(stderr,"WARN %s:%d\n",__FILE__,__LINE__),1):0)
#define container_of(ptr,type,member) ((type *)((char *)(ptr)-offsetof(type,member)))
typedef struct {atomic_int value;} atomic_t;
typedef struct {atomic_int_fast64_t value;} atomic64_t;
#define ATOMIC_INIT(v) {ATOMIC_VAR_INIT(v)}
#define ATOMIC64_INIT(v) {ATOMIC_VAR_INIT(v)}
static inline int atomic_inc_return(atomic_t *a){return atomic_fetch_add(&a->value,1)+1;}
static inline void atomic_dec(atomic_t *a){atomic_fetch_sub(&a->value,1);}
static inline void atomic64_set(atomic64_t *a,s64 value){atomic_store(&a->value,value);}
static inline s64 atomic64_read(atomic64_t *a){return atomic_load(&a->value);}
static inline bool atomic64_try_cmpxchg(atomic64_t *a,s64 *old,s64 new_value){return atomic_compare_exchange_weak(&a->value,old,new_value);}
typedef atomic_t refcount_t;
static inline void refcount_set(refcount_t *a,int value){atomic_store(&a->value,value);}
static inline void refcount_inc(refcount_t *a){assert(atomic_fetch_add(&a->value,1)>0);}
static inline bool refcount_inc_not_zero(refcount_t *a){int v=atomic_load(&a->value);while(v){if(atomic_compare_exchange_weak(&a->value,&v,v+1))return true;}return false;}
static inline void refcount_add(int n,refcount_t *a){assert(atomic_fetch_add(&a->value,n)>0);}
static inline bool refcount_dec_and_test(refcount_t *a){int old=atomic_fetch_sub(&a->value,1);assert(old>0);return old==1;}
typedef pthread_mutex_t spinlock_t;
#define DEFINE_SPINLOCK(name) pthread_mutex_t name=PTHREAD_MUTEX_INITIALIZER
#define DEFINE_MUTEX(name) pthread_mutex_t name=PTHREAD_MUTEX_INITIALIZER
#define mutex_lock pthread_mutex_lock
#define mutex_unlock(lock) pthread_mutex_unlock((pthread_mutex_t *)(lock))
#define spin_lock_init(lock) pthread_mutex_init(lock,NULL)
#define spin_lock pthread_mutex_lock
#define spin_unlock pthread_mutex_unlock
#define spin_lock_irqsave(lock,flags) do{(flags)=0;pthread_mutex_lock(lock);}while(0)
#define spin_unlock_irqrestore(lock,flags) do{(void)(flags);pthread_mutex_unlock(lock);}while(0)
struct rw_semaphore{pthread_rwlock_t lock;};
#define init_rwsem(s) pthread_rwlock_init(&(s)->lock,NULL)
#define down_read(s) pthread_rwlock_rdlock(&(s)->lock)
#define up_read(s) pthread_rwlock_unlock(&(s)->lock)
#define down_write(s) pthread_rwlock_wrlock(&(s)->lock)
#define up_write(s) pthread_rwlock_unlock(&(s)->lock)
struct list_head{struct list_head *next,*prev;};
#define LIST_HEAD(name) struct list_head name={&name,&name}
static inline void list_add_tail(struct list_head *entry,struct list_head *head){entry->prev=head->prev;entry->next=head;head->prev->next=entry;head->prev=entry;}
static inline void list_del_init(struct list_head *entry){entry->prev->next=entry->next;entry->next->prev=entry->prev;entry->prev=entry->next=entry;}
#define list_for_each_entry(item,head,field) for(struct list_head *node=(head)->next;node!=(head)&&((item)=container_of(node,__typeof__(*(item)),field),1);node=node->next)
struct module{atomic_int references;};
extern struct module native_module;
static inline bool try_module_get(struct module *m){if(m)atomic_fetch_add(&m->references,1);return true;}
static inline void module_put(struct module *m){if(m)assert(atomic_fetch_sub(&m->references,1)>0);}
#define kzalloc(size,flags) calloc(1,(size))
#define kfree free
#define ERR_PTR(error) ((void *)(intptr_t)(error))
#define IS_ERR(pointer) ((uintptr_t)(pointer)>=(uintptr_t)-4095)
#define PTR_ERR(pointer) ((intptr_t)(pointer))
extern atomic_size_t native_copy_fault_bytes;
static inline int copy_from_user(void *dst,const void *src,size_t n){size_t limit=atomic_exchange(&native_copy_fault_bytes,SIZE_MAX);size_t copied=n<limit?n:limit;memcpy(dst,src,copied);return n-copied;}
struct cgroup{int identity;};
struct task_struct{struct cgroup *group;};
extern struct task_struct native_task;
#define current (&native_task)
#define rcu_read_lock() ((void)0)
#define rcu_read_unlock() ((void)0)
static inline struct cgroup *task_dfl_cgroup(struct task_struct *task){return task->group;}
struct tio_device;
struct klet_grant_backing;
struct page{void *data;};
struct klet_inst{struct klet_grant_backing *grant_backing;struct {u64 grant_paddr,grant_len;} boot;pthread_mutex_t mutex;spinlock_t lock;bool started,dying;u32 nr_vcpus;struct cgroup *cgrp;refcount_t refs;struct tio_device *typedio_devices[8];u64 typedio_retired[8];struct klet_carrier *vcpus[8];};
struct klet_carrier{struct klet_inst *inst;unsigned int excess,vcpu;atomic_uint pending;};
struct klet_device{u32 type;int32_t fd;u32 flags,pad;};
static inline void klet_inst_get(struct klet_inst *inst){refcount_inc(&inst->refs);}
static inline void klet_inst_put(struct klet_inst *inst){assert(!refcount_dec_and_test(&inst->refs));}
void native_irq_prompt(void);
static inline void klet_virq_raise(struct klet_carrier *c,u32 bit){atomic_fetch_or(&c->pending,bit);native_irq_prompt();}
struct inode{int unused;};
struct file;
typedef unsigned int __poll_t;
typedef struct {int unused;} poll_table;
#define ERESTARTSYS 512
#define CAP_NET_BIND_SERVICE 10
#define capable(capability) true
#define mutex_init(lock) pthread_mutex_init(lock,NULL)
#define mutex_lock_interruptible(lock) pthread_mutex_lock(lock)
// Kernel sources name struct mutex; the mock has identical pthread behavior.
struct mutex { pthread_mutex_t native; };
#undef mutex_init
#undef mutex_lock_interruptible
#define mutex_init(lock) pthread_mutex_init(&(lock)->native,NULL)
#define mutex_lock_interruptible(lock) pthread_mutex_lock(&(lock)->native)
typedef struct wait_queue_head{pthread_mutex_t lock;pthread_cond_t changed;}wait_queue_head_t;
static inline void init_waitqueue_head(wait_queue_head_t *q){pthread_mutex_init(&q->lock,NULL);pthread_cond_init(&q->changed,NULL);}
static inline void wake_up_interruptible_all(wait_queue_head_t *q){pthread_mutex_lock(&q->lock);pthread_cond_broadcast(&q->changed);pthread_mutex_unlock(&q->lock);}
extern atomic_bool native_wait_interrupted;
void native_wait_register(wait_queue_head_t *queue);
void native_wait_unregister(wait_queue_head_t *queue);
#define wait_event_interruptible(q,condition) ({wait_queue_head_t *wq=&(q);int interrupted=0;native_wait_register(wq);pthread_mutex_lock(&wq->lock);while(!(condition)){if(atomic_exchange(&native_wait_interrupted,false)){interrupted=-ERESTARTSYS;break;}pthread_cond_wait(&wq->changed,&wq->lock);}pthread_mutex_unlock(&wq->lock);native_wait_unregister(wq);interrupted;})
#define poll_wait(file,queue,table) ((void)(file),(void)(queue),(void)(table))
struct file_operations{struct module *owner;long(*unlocked_ioctl)(struct file *,unsigned int,unsigned long);ssize_t(*read)(struct file *,char *,size_t,loff_t *);ssize_t(*write)(struct file *,const char *,size_t,loff_t *);__poll_t(*poll)(struct file *,poll_table *);int(*release)(struct inode *,struct file *);long(*llseek)(void);};
struct file{int f_flags;refcount_t refs;const struct file_operations *f_op;void *private_data;};
long noop_llseek(void);
int anon_inode_getfd(const char *,const struct file_operations *,void *,int);
struct file *fget(int);
void fput(struct file *);
void native_close(int);
struct work_struct{void(*function)(struct work_struct *);atomic_bool pending;};
struct workqueue_struct;
#define INIT_WORK(work,fn) do{(work)->function=(fn);atomic_init(&(work)->pending,false);}while(0)
struct workqueue_struct *alloc_workqueue(const char *,int,int);
bool queue_work(struct workqueue_struct *,struct work_struct *);
void destroy_workqueue(struct workqueue_struct *);
#include "../../linux/endovisor/typedio_bounds.h"
#define lockdep_assert_held(lock) ((void)(lock))
static inline bool klet_in_grant(struct klet_inst *i,u64 p,u64 n){return klet_range_contains(i->boot.grant_paddr,i->boot.grant_len,p,n);}
static inline unsigned int get_order(u64 bytes){unsigned int order=0;while((PAGE_SIZE<<order)<bytes)order++;return order;}
struct page *alloc_pages(int,unsigned int);
void __free_pages(struct page *,unsigned int);
static inline void *page_address(struct page *p){return p->data;}
static inline u64 page_to_phys(struct page *p){return (uintptr_t)p->data;}
static inline int copy_to_user(void *dst,const void *src,size_t n){return copy_from_user(dst,src,n);}
#define min_t(t,a,b) ((t)(a)<(t)(b)?(t)(a):(t)(b))
#define min(a,b) ((a)<(b)?(a):(b))
static inline void atomic64_sub(u64 n,atomic64_t *a){atomic_fetch_sub(&a->value,n);}
struct klet_grant_backing *klet_grant_alloc(unsigned int);
struct page *klet_grant_pages(struct klet_grant_backing *);
struct klet_grant_backing *klet_grant_lease_get(struct klet_inst *,u64,u64);
void klet_grant_put(struct klet_grant_backing *);
void klet_grant_lease_put(struct klet_grant_backing *);

#endif
