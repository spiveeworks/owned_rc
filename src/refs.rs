use std::cell;
use std::mem;
use std::ops;
use std::ptr;
use std::rc;

struct RawRc<T: ?Sized>(*const T);

impl<T: ?Sized> RawRc<T> {
    fn new(ptr: rc::Rc<T>) -> Self {
        RawRc(rc::Rc::into_raw(ptr))
    }

    // make sure that this unbounded object doesn't actually
    //   outlive the Rc family
    // recommended that the unbounded object and the RawRc are stored
    //   adjacent to eachother somehow
    unsafe fn unbounded<'a, 'b>(&'a self) -> &'b T {
        unsafe {
            &*self.0
        }
    }
}

impl<T: ?Sized> Clone for RawRc<T> {
    fn clone(&self) -> Self {
        unsafe {
            let ptr = rc::Rc::from_raw(self.0);
            let result = RawRc::new(rc::Rc::clone(&ptr));
            mem::forget(ptr);
            result
        }
    }
}


impl<T: ?Sized> Drop for RawRc<T> {
    fn drop(&mut self) {
        unsafe {
            mem::drop(rc::Rc::from_raw(self.0))
        }
    }
}



trait RefTrait<T: ?Sized>: Sized {
    type Error;
    fn try_borrow(&cell::RefCell<T>) -> Result<Self, Self::Error>;
}

impl<'a, T: 'a + ?Sized> RefTrait<T> for cell::Ref<'a, T> {
    type Error = cell::BorrowError;
    fn try_borrow(ref_cell: &cell::RefCell<T>) -> Result<Self, Self::Error> {
        ref_cell.try_borrow()
    }
}

impl<'a, T: 'a + ?Sized> RefTrait<T> for cell::RefMut<'a, T> {
    type Error = cell::BorrowMutError;
    fn try_borrow(ref_cell: &cell::RefCell<T>) -> Result<Self, Self::Error> {
        ref_cell.try_borrow_mut()
    }
}

// marker trait for RefInner
trait RefDeref:
    ops::Deref + RefTrait<<Self as ops::Deref>::Target> {}
// RefInner can wrap types that act like cell::Ref, and implement Deref
impl<RefT> RefDeref for RefT where RefT:
    ops::Deref + RefTrait<<RefT as ops::Deref>::Target> {}

#[derive(Clone)]
struct RefInner<Brw> where Brw: RefDeref
{
    borrow: mem::ManuallyDrop<Brw>,
    strong: RawRc<cell::RefCell<Brw::Target>>,
}

impl<Brw> RefInner<Brw>
    where Brw: RefDeref
{
    fn new(ptr: rc::Rc<cell::RefCell<Brw::Target>>)
        -> Result<Self, <Brw as RefTrait< <Brw as ops::Deref>::Target> >::Error>
    {
        // if this function panics this will be dropped after the Ref,
        // although cell::Ref and cell::RefMut shouldn't panic anyway
        let strong = RawRc::new(ptr);
        unsafe {
            // note that because of this call to .unbounded,
            //   we now need to fret over the drop order of this structs
            //   fields...
            let ref_cell = strong.unbounded();
            let borrow = RefTrait::try_borrow(&ref_cell)?;
            RefInner {
                borrow: mem::ManuallyDrop::new(borrow),
                strong,
            }
        }
    }

    fn inner(&self) -> &Brw { self.borrow.as_ref().unwrap() }
    fn inner_mut(&mut self) -> &mut Brw { self.borrow.as_mut().unwrap() }

    fn map<MBrw, F>(self: RefInner<Brw>, f: F) -> RefInner<MBrw>
        where F: FnOnce(Brw) -> MBrw,
              MBrw: RefDeref,
    {
        unsafe {
            // how to destructure a Drop type
            let strong = ptr::read(&self.strong);
            let borrow_wrapped = ptr::read(&self.borrow);

            // make sure this is declared after strong
            //   so that it is dropped before
            let borrow = mem::ManuallyDrop::into_inner(borrow_wrapped);

            // make sure that this is leaked before we call the closure
            mem::forget(self);

            let mapped_borrow = f(borrow);
            let final_borrow = mem::ManuallyDrop::new(mapped_borrow);
            RefInner {
                strong,
                borrow: final_borrow,
            }
        }
    }
}

impl<Brw> Drop for RefInner<Brw>
    where Brw: RefDeref
{
    fn drop(&mut self) {
        // drop the unbounded ref first, before the RawRc is dropped
        unsafe {
            mem::ManuallyDrop::drop(self.borrow);
        }
    }
}




pub struct Ref<'a, T: 'a> {
    inner: RefInner<cell::Ref<'a, T>>
}

pub struct RefMut<'a, T: 'a> {
    inner: RefInner<cell::RefMut<'a, T>>
}

impl<'a, T: 'a> ops::Deref for Ref<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner.inner()
    }
}

impl<'a, T: 'a> ops::Deref for RefMut<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner.inner()
    }
}

impl<'a, T: 'a> ops::DerefMut for RefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner.inner_mut()
    }
}

impl<'a, T: 'a> Ref<'a, T> {
    pub(super) unsafe fn new(ptr: rc::Rc<cell::RefCell<T>>)
        -> Result<Self, cell::BorrowError>
    {
        Ref { inner: RefInner::new(ptr) }
    }

    pub fn clone(this: &Self) -> Self {
        Ref { inner: this.inner.clone() }
    }

    pub fn map<F, U>(this: Ref<'a, T>, f: F) -> Ref<'a, U>
        where F: FnOnce(&T) -> &U
    {
        let inner = this.inner.map(|cell_ref| {
            cell::Ref::map(cell_ref, f)
        });
        Ref { inner }
    }
}

impl<'a, T: 'a> RefMut<'a, T> {
    pub(super) unsafe fn new(ptr: rc::Rc<cell::RefCell<T>>)
        -> Result<Self, cell::BorrowMutError>
    {
        RefMut { inner: RefInner::new(ptr) }
    }

    pub fn map<F, U>(this: RefMut<'a, T>, f: F) -> RefMut<'a, U>
        where F: FnOnce(&mut T) -> &mut U
    {
        let inner = this.inner.map(|cell_ref| {
            cell::RefMut::map(cell_ref, f)
        });
        Ref { inner }
    }
}
