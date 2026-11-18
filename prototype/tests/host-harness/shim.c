// SPDX-License-Identifier: GPL-2.0
#include "shim.h"
struct module native_module;
struct task_struct native_task;
static pthread_mutex_t fd_lock=PTHREAD_MUTEX_INITIALIZER;
static struct file *files[256];
long noop_llseek(void){return -ESPIPE;}
static atomic_bool native_fd_failure;
int anon_inode_getfd(const char *name,const struct file_operations *ops,void *data,int flags){
    if(atomic_exchange(&native_fd_failure,false))return -EMFILE;
    (void)name;struct file *f=calloc(1,sizeof(*f));if(!f)return -ENOMEM;
    f->f_flags=flags;refcount_set(&f->refs,1);f->f_op=ops;f->private_data=data;assert(try_module_get(ops->owner));
    pthread_mutex_lock(&fd_lock);
    for(int i=3;i<256;i++)if(!files[i]){files[i]=f;pthread_mutex_unlock(&fd_lock);return i;}
    pthread_mutex_unlock(&fd_lock);module_put(ops->owner);free(f);return -EMFILE;
}
struct file *fget(int fd){pthread_mutex_lock(&fd_lock);struct file *f=fd>=0&&fd<256?files[fd]:NULL;if(f)refcount_inc(&f->refs);pthread_mutex_unlock(&fd_lock);return f;}
void fput(struct file *f){if(refcount_dec_and_test(&f->refs)){f->f_op->release(NULL,f);module_put(f->f_op->owner);free(f);}}
void native_close(int fd){pthread_mutex_lock(&fd_lock);struct file *f=files[fd];files[fd]=NULL;pthread_mutex_unlock(&fd_lock);if(f)fput(f);}
struct workqueue_struct{pthread_mutex_t lock;pthread_cond_t condition;pthread_t threads[8];struct work_struct *ring[2048];size_t head,len,running;bool stopping;};
static void *worker(void *argument){
    struct workqueue_struct *pool=argument;
    for(;;){
        pthread_mutex_lock(&pool->lock);
        while(!pool->len&&!pool->stopping)pthread_cond_wait(&pool->condition,&pool->lock);
        if(pool->stopping){pthread_mutex_unlock(&pool->lock);return NULL;}
        struct work_struct *work=pool->ring[pool->head];pool->head=(pool->head+1)%2048;pool->len--;pool->running++;
        void(*fn)(struct work_struct *)=work->function;atomic_store(&work->pending,false);
        pthread_mutex_unlock(&pool->lock);fn(work);
        // Work/container may have been freed by fn. Never dereference it here.
        pthread_mutex_lock(&pool->lock);pool->running--;pthread_cond_broadcast(&pool->condition);pthread_mutex_unlock(&pool->lock);
    }
}
struct workqueue_struct *alloc_workqueue(const char *name,int flags,int maximum){
    (void)name;(void)flags;assert(maximum==8);struct workqueue_struct *p=calloc(1,sizeof(*p));if(!p)return NULL;
    pthread_mutex_init(&p->lock,NULL);pthread_cond_init(&p->condition,NULL);
    for(int i=0;i<8;i++)assert(!pthread_create(&p->threads[i],NULL,worker,p));return p;
}
bool queue_work(struct workqueue_struct *p,struct work_struct *work){
    bool expected=false;if(!atomic_compare_exchange_strong(&work->pending,&expected,true))return false;
    pthread_mutex_lock(&p->lock);assert(p->len<2048);p->ring[(p->head+p->len)%2048]=work;p->len++;pthread_cond_signal(&p->condition);pthread_mutex_unlock(&p->lock);return true;
}
void destroy_workqueue(struct workqueue_struct *p){
    pthread_mutex_lock(&p->lock);while(p->len||p->running)pthread_cond_wait(&p->condition,&p->lock);
    p->stopping=true;pthread_cond_broadcast(&p->condition);pthread_mutex_unlock(&p->lock);
    for(int i=0;i<8;i++)pthread_join(p->threads[i],NULL);pthread_cond_destroy(&p->condition);pthread_mutex_destroy(&p->lock);free(p);
}

struct page *alloc_pages(int flags,unsigned int order){
    assert(flags&__GFP_ZERO);struct page *p=calloc(1,sizeof(*p));if(!p)return NULL;
    if(posix_memalign(&p->data,PAGE_SIZE,PAGE_SIZE<<order)){free(p);return NULL;}
    memset(p->data,0,PAGE_SIZE<<order);return p;
}
void __free_pages(struct page *p,unsigned int order){(void)order;free(p->data);free(p);}

int native_host_accept(int fd){struct file *f=fget(fd);assert(f);int r=f->f_op->unlocked_ioctl(f,KLET_TIO_ACCEPT,O_NONBLOCK|O_CLOEXEC);fput(f);return r;}
ssize_t native_host_read(int fd,void *buffer,size_t n){struct file *f=fget(fd);assert(f);ssize_t r=f->f_op->read(f,buffer,n,NULL);fput(f);return r;}
ssize_t native_host_write(int fd,const void *buffer,size_t n){struct file *f=fget(fd);assert(f);ssize_t r=f->f_op->write(f,buffer,n,NULL);fput(f);return r;}
u32 native_host_poll(int fd){struct file *f=fget(fd);assert(f);u32 r=f->f_op->poll(f,NULL);fput(f);return r;}

__attribute__((weak)) s64 native_guest_service(struct klet_tio_call *call){(void)call;return -ENODEV;}
__attribute__((weak)) void native_select_guest(u32 index){(void)index;}

atomic_size_t native_copy_fault_bytes=ATOMIC_VAR_INIT(SIZE_MAX);
void native_set_copy_fault(size_t bytes){atomic_store(&native_copy_fault_bytes,bytes);}

void native_fail_next_fd(void) {atomic_store(&native_fd_failure, true);}
struct native_async_read {pthread_t thread;int fd;char buffer[32];ssize_t result;};
static struct native_async_read native_reader;
static void *native_async_read_run(void *argument)
{struct native_async_read *reader=argument;reader->result=native_host_read(reader->fd,reader->buffer,sizeof(reader->buffer));return NULL;}
void native_start_blocking_read(int fd)
{
    struct file *file=fget(fd);assert(file);file->f_flags &= ~O_NONBLOCK;fput(file);
    native_reader.fd=fd;assert(!pthread_create(&native_reader.thread,NULL,native_async_read_run,&native_reader));
}
ssize_t native_join_blocking_read(void *buffer,size_t size)
{
    assert(!pthread_join(native_reader.thread,NULL));assert(native_reader.result<0||(size_t)native_reader.result<=size);
    if(native_reader.result>0)memcpy(buffer,native_reader.buffer,native_reader.result);
    struct file *file=fget(native_reader.fd);assert(file);file->f_flags|=O_NONBLOCK;fput(file);
    return native_reader.result;
}

__attribute__((weak)) int native_drain_irq_sources(void){return 0;}

atomic_bool native_wait_interrupted;
static pthread_mutex_t native_wait_lock=PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t native_wait_ready=PTHREAD_COND_INITIALIZER;
static wait_queue_head_t *native_waiting;
void native_wait_register(wait_queue_head_t *queue){pthread_mutex_lock(&native_wait_lock);assert(!native_waiting);native_waiting=queue;pthread_cond_broadcast(&native_wait_ready);pthread_mutex_unlock(&native_wait_lock);}
void native_wait_unregister(wait_queue_head_t *queue){pthread_mutex_lock(&native_wait_lock);assert(native_waiting==queue);native_waiting=NULL;pthread_mutex_unlock(&native_wait_lock);}
void native_interrupt_blocking_read(void){
    pthread_mutex_lock(&native_wait_lock);while(!native_waiting)pthread_cond_wait(&native_wait_ready,&native_wait_lock);
    atomic_store(&native_wait_interrupted,true);wake_up_interruptible_all(native_waiting);pthread_mutex_unlock(&native_wait_lock);
}
void native_wait_for_blocking_read(void){pthread_mutex_lock(&native_wait_lock);while(!native_waiting)pthread_cond_wait(&native_wait_ready,&native_wait_lock);pthread_mutex_unlock(&native_wait_lock);}
