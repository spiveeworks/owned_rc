use std::cell;
use std::marker;
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
        &*self.0
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



trait RefTrait<'a, T: 'a + ?Sized>: Sized {
    type Error;
    fn try_borrow(&'a cell::RefCell<T>)
        -> Result<Self, Self::Error>;
}

impl<'a, T: 'a + ?Sized> RefTrait<'a, T> for cell::Ref<'a, T> {
    type Error = cell::BorrowError;
    fn try_borrow(ref_cell: &'a cell::RefCell<T>)
        -> Result<Self, Self::Error>
    {
        ref_cell.try_borrow()
    }
}

impl<'a, T: 'a + ?Sized> RefTrait<'a, T> for cell::RefMut<'a, T> {
    type Error = cell::BorrowMutError;
    fn try_borrow(ref_cell: &'a cell::RefCell<T>)
        -> Result<Self, Self::Error>
    {
        ref_cell.try_borrow_mut()
    }
}


// aliases to make type definitions simpler
type DerefT<T> = <T as ops::Deref>::Target;
type RefDerefError<'a, T> =
    <T as RefTrait<'a, DerefT<T>>>::Error;

// marker trait for RefInner
trait RefDeref<'a>:
    ops::Deref + RefTrait<'a, DerefT<Self>>
    where DerefT<Self>: 'a {}
// RefInner can wrap types that act like cell::Ref, and implement Deref
impl<'a, RefT: 'a> RefDeref<'a> for RefT
    where RefT: ops::Deref + RefTrait<'a, DerefT<Self>>,
          DerefT<Self>: 'a {}

struct RefInner<'a, Brw, T = <Brw as ops::Deref>::Target>
    where Brw: 'a + RefDeref<'a>,
          T: 'a + ?Sized,
{
    borrow: mem::ManuallyDrop<Brw>,
    // the strong has an independent type,
    //   because of cell::Ref::map
    strong: RawRc<cell::RefCell<T>>,
    _doesnt_outlive_self: marker::PhantomData<&'a cell::RefCell<T>>,
}

/* about the lifetime parameter....
 *
 * RefInner is bounded by the RefCell that it is keeping alive
 * normally Brw will have a lifetime parameter that the RefCell must outlive
 *
 * so RefInner has to have a lifetime parameter to specify what kind of
 * RefCell its Brw term was borrowed from...
 * BUT RefInner actually generates an unbounded RefCell object
 *     by effectively using rc::Rc::into_raw
 *
 * so ultimately it has constraints saying that it doesn't outlive:
 *  1. the data inside the refcell that it keeps alive
 *  2. the refcell itself
 * which it can't do... since it actually owns those things!
 *
 */


impl<'a, Brw> RefInner<'a, Brw>
    where Brw: RefDeref<'a>
{
    fn new(ptr: rc::Rc<cell::RefCell<Brw::Target>>)
        -> Result<Self, RefDerefError<'a, Brw>>
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
            let result = RefInner {
                borrow: mem::ManuallyDrop::new(borrow),
                strong,
                _doesnt_outlive_self: marker::PhantomData,
            };
            Ok(result)
        }
    }
}

trait RefClone {
    fn ref_clone(&self) -> Self;
}

impl<'a, T> RefClone for cell::Ref<'a, T> {
    fn ref_clone(&self) -> Self { cell::Ref::clone(self) }
}

impl<'a, Brw, T> RefClone for RefInner<'a, Brw, T>
    where Brw: RefDeref<'a> + RefClone,
          T: ?Sized
{
    fn ref_clone(&self) -> Self {
        RefInner {
            strong: self.strong.clone(),
            borrow: mem::ManuallyDrop::new(self.borrow.ref_clone()),
            _doesnt_outlive_self: marker::PhantomData,
        }
    }
}

impl<'a, Brw, T> RefInner<'a, Brw, T>
    where Brw: RefDeref<'a>,
          T: 'a + ?Sized
{


    fn inner(&self) -> &Brw { &*self.borrow }
    fn inner_mut(&mut self) -> &mut Brw { &mut *self.borrow }

    fn map<MBrw, F>(self: RefInner<'a, Brw, T>, f: F) -> RefInner<'a, MBrw, T>
        where F: FnOnce(Brw) -> MBrw,
              MBrw: RefDeref<'a>,
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
                _doesnt_outlive_self: marker::PhantomData,
            }
        }
    }
}

impl<'a, Brw, T> Drop for RefInner<'a, Brw, T>
    where Brw: RefDeref<'a>,
          T: 'a + ?Sized
{
    fn drop(&mut self) {
        // drop the unbounded ref first, before the RawRc is dropped
        unsafe {
            mem::ManuallyDrop::drop(&mut self.borrow);
        }
    }
}



// T is the type enclosed, ST is the type originally borrowed
// we need both, as Ref might end up dropping the borrowed data

pub struct Ref<'a, T: 'a, ST: 'a = T> {
    inner: RefInner<'a, cell::Ref<'a, T>, ST>
}

pub struct RefMut<'a, T: 'a, ST: 'a = T> {
    inner: RefInner<'a, cell::RefMut<'a, T>, ST>
}

impl<'a, T: 'a, ST: 'a> ops::Deref for Ref<'a, T, ST> {
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
    // note this produces an unbounded object
    pub(super) fn new(ptr: rc::Rc<cell::RefCell<T>>)
        -> Result<Self, cell::BorrowError>
    {
        RefInner::new(ptr).map(|inner| Ref { inner })
    }
}

impl<'a, T: 'a, ST: 'a> Ref<'a, T, ST> {
    pub fn clone(this: &Self) -> Self {
        Ref { inner: this.inner.ref_clone() }
    }

    pub fn map<F, U>(this: Ref<'a, T, ST>, f: F) -> Ref<'a, U, ST>
        where F: FnOnce(&T) -> &U
    {
        let inner = this.inner.map(|cell_ref| {
            cell::Ref::map(cell_ref, f)
        });
        Ref { inner }
    }
}

impl<'a, T: 'a> RefMut<'a, T> {
    // note this produces an unbounded object
    pub(super) fn new(ptr: rc::Rc<cell::RefCell<T>>)
        -> Result<Self, cell::BorrowMutError>
    {
        RefInner::new(ptr).map(|inner| RefMut { inner })
    }
}

impl<'a, T: 'a, ST: 'a> RefMut<'a, T, ST> {
    pub fn map<F, U>(this: RefMut<'a, T, ST>, f: F) -> RefMut<'a, U, ST>
        where F: FnOnce(&mut T) -> &mut U
    {
        let inner = this.inner.map(|cell_ref| {
            cell::RefMut::map(cell_ref, f)
        });
        RefMut { inner }
    }
}
