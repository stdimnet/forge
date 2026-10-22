#[cfg(feature = "parallel")]
macro_rules! par_iter {
    ($e:expr) => {
        rayon::prelude::IntoParallelRefIterator::par_iter(&$e)
    };
}
#[cfg(not(feature = "parallel"))]
macro_rules! par_iter {
    ($e:expr) => {
        $e.iter()
    };
}

#[cfg(feature = "parallel")]
macro_rules! par_chunks {
    ($e:expr, $n:expr) => {
        rayon::prelude::ParallelSlice::par_chunks(&$e[..], $n)
    };
}
#[cfg(not(feature = "parallel"))]
macro_rules! par_chunks {
    ($e:expr, $n:expr) => {
        $e.chunks($n)
    };
}

#[cfg(feature = "parallel")]
macro_rules! par_iter_mut {
    ($e:expr) => {
        rayon::prelude::IntoParallelRefMutIterator::par_iter_mut(&mut $e)
    };
}
#[cfg(not(feature = "parallel"))]
macro_rules! par_iter_mut {
    ($e:expr) => {
        $e.iter_mut()
    };
}
