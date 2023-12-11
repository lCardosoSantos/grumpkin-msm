// Copyright Supranational LLC
// Licensed under the Apache License, Version 2.0, see LICENSE for details.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate as grumpkin_msm;
    use core::cell::UnsafeCell;
    use core::mem::transmute;
    use core::sync::atomic::*;
    use halo2curves::{
        CurveExt,
        group::{ff::Field, Curve},
        bn256,
    };
    use rand::{RngCore, SeedableRng};
    use rand_chacha::ChaCha20Rng;

    pub fn gen_points(npoints: usize) -> Vec<bn256::G1Affine> {
        let mut ret: Vec<bn256::G1Affine> = Vec::with_capacity(npoints);
        unsafe { ret.set_len(npoints) };

        let mut rnd: Vec<u8> = Vec::with_capacity(32 * npoints);
        unsafe { rnd.set_len(32 * npoints) };
        ChaCha20Rng::from_entropy().fill_bytes(&mut rnd);

        let n_workers = rayon::current_num_threads();
        let work = AtomicUsize::new(0);
        rayon::scope(|s| {
            for _ in 0..n_workers {
                s.spawn(|_| {
                let hash = bn256::G1::hash_to_curve("foobar");

                let mut stride = 1024;
                let mut tmp: Vec<bn256::G1> = Vec::with_capacity(stride);
                unsafe { tmp.set_len(stride) };

                loop {
                    let work = work.fetch_add(stride, Ordering::Relaxed);
                    if work >= npoints {
                        break;
                    }
                    if work + stride > npoints {
                        stride = npoints - work;
                        unsafe { tmp.set_len(stride) };
                    }
                    for i in 0..stride {
                        let off = (work + i) * 32;
                        tmp[i] = hash(&rnd[off..off + 32]);
                    }
                    #[allow(mutable_transmutes)]
                    bn256::G1::batch_normalize(&tmp, unsafe {
                        transmute::<&[bn256::G1Affine], &mut [bn256::G1Affine]>(
                            &ret[work..work + stride],
                        )
                    });
                }
            })
            }
        });

        ret
    }

    fn as_mut<T>(x: &T) -> &mut T {
        unsafe { &mut *UnsafeCell::raw_get(x as *const _ as *const _) }
    }

    pub fn gen_scalars(npoints: usize) -> Vec<bn256::Fr> {
        let mut ret: Vec<bn256::Fr> = Vec::with_capacity(npoints);
        unsafe { ret.set_len(npoints) };

        let n_workers = rayon::current_num_threads();
        let work = AtomicUsize::new(0);

        rayon::scope(|s| {
            for _ in 0..n_workers {
                s.spawn(|_| {
                    let mut rng = ChaCha20Rng::from_entropy();
                    loop {
                        let work = work.fetch_add(1, Ordering::Relaxed);
                        if work >= npoints {
                            break;
                        }
                        *as_mut(&ret[work]) = bn256::Fr::random(&mut rng);
                    }
                })
            }
        });

        ret
    }

    pub fn naive_multiscalar_mul(
        points: &[bn256::G1Affine],
        scalars: &[bn256::Fr],
    ) -> bn256::G1Affine {
        let n_workers = rayon::current_num_threads();

        let mut rets: Vec<bn256::G1> = Vec::with_capacity(n_workers);
        unsafe { rets.set_len(n_workers) };

        let npoints = points.len();
        let work = AtomicUsize::new(0);
        let tid = AtomicUsize::new(0);
        rayon::scope(|s| {
            for _ in 0..n_workers {
                s.spawn(|_| {
                    let mut ret = bn256::G1::default();

                    loop {
                        let work = work.fetch_add(1, Ordering::Relaxed);
                        if work >= npoints {
                            break;
                        }
                        ret += points[work] * scalars[work];
                    }

                    *as_mut(&rets[tid.fetch_add(1, Ordering::Relaxed)]) = ret;
                })
            }
        });

        let mut ret = bn256::G1::default();
        for i in 0..n_workers {
            ret += rets[i];
        }

        ret.to_affine()
    }

    #[test]
    fn it_works() {
        #[cfg(not(debug_assertions))]
        const NPOINTS: usize = 128 * 1024;
        #[cfg(debug_assertions)]
        const NPOINTS: usize = 8 * 1024;

        let points = gen_points(NPOINTS);
        let scalars = gen_scalars(NPOINTS);

        let naive = naive_multiscalar_mul(&points, &scalars);
        println!("{:?}", naive);

        let ret = grumpkin_msm::bn256(&points, &scalars).to_affine();
        println!("{:?}", ret);

        assert_eq!(ret, naive);
    }
}
