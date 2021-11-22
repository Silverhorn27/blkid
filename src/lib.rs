// Copyright (c) 2017 Chris Holcombe

// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT> This file may not be copied, modified,
// or distributed except according to those terms.

//! See https://www.kernel.org/pub/linux/utils/util-linux/v2.21/libblkid-docs/index.html
//! for the reference manual to the FFI bindings
extern crate blkid_sys;
extern crate errno;
extern crate libc;

pub mod cache;
pub mod dev;
mod error;
pub mod tag;

pub use error::BlkidError;

use crate::error::{result, result_ptr_mut};
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    path::Path,
    ptr,
};

use blkid_sys::*;

pub struct BlkId {
    probe: blkid_probe,
}

impl BlkId {
    pub fn new(file: &Path) -> Result<BlkId, BlkidError> {
        let path = CString::new(file.as_os_str().to_string_lossy().as_ref())?;
        let probe = result_ptr_mut(unsafe {
            // pub fn blkid_do_probe(pr: blkid_probe) -> ::std::os::raw::c_int;
            // pub fn blkid_do_fullprobe(pr: blkid_probe) -> ::std::os::raw::c_int;
            blkid_new_probe_from_filename(path.as_ptr())
        })?;

        Ok(BlkId { probe })
    }

    /// Calls probing functions in all enabled chains. The superblocks chain is enabled by
    /// default. The `blkid_do_probe()` stores result from only one probing function.
    /// It's necessary to call this routine in a loop to get results from all probing functions
    /// in all chains. The probing is reset by `blkid_reset_probe()` or by filter functions.
    /// This is string-based NAME=value interface only.
    pub fn do_probe(&self) -> Result<i32, BlkidError> {
        let ret_code = unsafe { blkid_do_probe(self.probe) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(ret_code)
    }

    /// This function gathers probing results from all enabled chains and checks for ambivalent
    /// results (e.g. more filesystems on the device).
    ///
    /// This is string-based `NAME=value` interface only.
    ///
    /// # Note
    /// About suberblocks chain -- the function does not check for filesystems when a
    /// RAID signature is detected. The function also does not check for collision between RAIDs.
    /// The first detected RAID is returned. The function checks for collision between partition
    /// table and RAID signature -- it's recommended to enable partitions chain together with
    /// superblocks chain.
    ///
    /// # Returns
    /// `Ok(0)` on success, `Ok(1)` on success and nothing was detected, `Ok(-2)` if the
    /// probe was ambivalent.
    pub fn do_safe_probe(&self) -> Result<i32, BlkidError> {
        let ret_code = unsafe { blkid_do_safeprobe(self.probe) };
        if ret_code == -1 {
            return Err(BlkidError::get_error());
        }

        Ok(ret_code)
    }

    /// Value by specified `name`
    ///
    /// # Note
    /// You should call [`Self::prob`] before using this
    pub fn lookup_value(&self, name: &str) -> Result<String, BlkidError> {
        let name = CString::new(name)?;
        let mut data_ptr: *const ::libc::c_char = ptr::null();
        let mut len = 0;
        let ret_code =
            unsafe { blkid_probe_lookup_value(self.probe, name.as_ptr(), &mut data_ptr, &mut len) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        let data_value = unsafe { CStr::from_ptr(data_ptr as *const ::libc::c_char) };
        Ok(data_value.to_string_lossy().into_owned())
    }

    /// Check if device has the specified value
    pub fn has_value(&self, name: &str) -> Result<bool, BlkidError> {
        let name = CString::new(name)?;
        let ret_code = unsafe { blkid_probe_has_value(self.probe, name.as_ptr()) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        match ret_code {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(BlkidError::LibBlkidHasValue { ret_code }),
        }
    }

    /// Number of values in probing result
    pub fn numof_values(&self) -> Result<i32, BlkidError> {
        let ret_code = unsafe { blkid_probe_numof_values(self.probe) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(ret_code)
    }

    /// Retrieve the Nth item `(Name, Value)` in the probing result, (0..self.numof_values())
    pub fn get_value(&self, num: i32) -> Result<(String, String), BlkidError> {
        let mut name_ptr: *const ::libc::c_char = ptr::null();
        let mut data_ptr: *const ::libc::c_char = ptr::null();
        let mut len = 0;

        let ret_code = unsafe {
            blkid_probe_get_value(self.probe, num, &mut name_ptr, &mut data_ptr, &mut len)
        };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        let name_value = unsafe { CStr::from_ptr(name_ptr as *const ::libc::c_char) };
        let data_value = unsafe { CStr::from_ptr(data_ptr as *const ::libc::c_char) };
        Ok((
            name_value.to_string_lossy().into_owned(),
            data_value.to_string_lossy().into_owned(),
        ))
    }

    // https://github.com/karelzak/util-linux/blob/master/Documentation/blkid.txt
    /// Retrieve the value of a specific attribute for a particular device.  
    /// This can be used to determine attributes such as TYPE, UUID, LABEL, and PARTUUID
    ///
    /// # Returns
    /// Empty string if the requested attribute is not set for a particular device
    pub fn get_tag_value(&self, tagname: &str, devname: &Path) -> Result<String, BlkidError> {
        let tag_name = CString::new(tagname)?;
        let dev_name = CString::new(devname.as_os_str().to_string_lossy().as_ref())?;
        let cache: blkid_cache = ptr::null_mut();

        let ret_value: *mut ::libc::c_char =
            unsafe { blkid_get_tag_value(cache, tag_name.as_ptr(), dev_name.as_ptr()) };
        if ret_value.is_null() {
            return Ok("".to_string());
        }

        let data_value = unsafe { CString::from_raw(ret_value) };
        Ok(data_value.into_string()?)
    }

    /// Retrieve a HashMap of all the probed values
    pub fn get_values_map(&self) -> Result<HashMap<String, String>, BlkidError> {
        Ok((0..self.numof_values()?)
            .map(|i| self.get_value(i).expect("'i' is in range"))
            .collect())
    }

    /// Block device number, or 0 for regular files
    pub fn get_devno(&self) -> u64 {
        unsafe { blkid_probe_get_devno(self.probe) }
    }

    /// Device number of the wholedisk, or 0 for regular files
    pub fn get_wholedisk_devno(&self) -> u64 {
        unsafe { blkid_probe_get_wholedisk_devno(self.probe) }
    }

    /// If the device is whole-disk
    pub fn is_wholedisk(&self) -> Result<bool, BlkidError> {
        let ret_code = unsafe { blkid_probe_is_wholedisk(self.probe) };

        match ret_code {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(BlkidError::LibBlkidHasValue { ret_code }),
        }
    }

    /// The size of probing area.
    /// If the size is unrestricted then this function returns the real size of device
    pub fn get_size(&self) -> Result<i64, BlkidError> {
        let ret_code = unsafe { blkid_probe_get_size(self.probe) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(ret_code)
    }

    /// Offset of probing area
    pub fn get_offset(&self) -> Result<i64, BlkidError> {
        let ret_code = unsafe { blkid_probe_get_offset(self.probe) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(ret_code)
    }

    /// Block device logical sector size (`BLKSSZGET` ioctl, default 512)
    pub fn get_sectorsize(&self) -> u32 {
        unsafe { blkid_probe_get_sectorsize(self.probe) }
    }

    /// 512-byte sector count
    pub fn get_sectors(&self) -> Result<i64, BlkidError> {
        let ret_code = unsafe { blkid_probe_get_sectors(self.probe) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(ret_code)
    }

    /// File descriptor for assigned device/file
    pub fn get_fd(&self) -> Result<i32, BlkidError> {
        unsafe { Ok(blkid_probe_get_fd(self.probe)) }
    }

    /// If known filesystem
    pub fn known_fstype(&self, fstype: &str) -> Result<bool, BlkidError> {
        let fstype = CString::new(fstype)?;
        let ret_code = unsafe { blkid_known_fstype(fstype.as_ptr()) };

        match ret_code {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(BlkidError::LibBlkidKnownFsType { ret_code }),
        }
    }

    /// Enables/disables the topology probing for non-binary interface.
    pub fn enable_topology(&self) -> Result<(), BlkidError> {
        let ret_code = unsafe { blkid_probe_enable_topology(self.probe, 1) };
        if ret_code < 0 {
            Err(BlkidError::get_error())
        } else {
            Ok(())
        }
    }

    /// This is a binary interface for topology values. See also blkid_topology_* functions.
    /// This function is independent on blkid_do_[safe,full]probe() and
    /// blkid_probe_enable_topology() calls.
    /// WARNING: the returned object will be overwritten by the next blkid_probe_get_topology()
    /// call for the same pr. If you want to use more blkid_topopogy objects in the same time you
    /// have to create more blkid_probe handlers (see blkid_new_probe()).
    pub fn get_topology(&self) -> Result<blkid_topology, BlkidError> {
        unsafe { Ok(blkid_probe_get_topology(self.probe)) }
    }

    /// Alignment offset in bytes or 0.
    pub fn get_topology_alignment_offset(tp: blkid_topology) -> u64 {
        unsafe { blkid_topology_get_alignment_offset(tp) }
    }

    /// Minimum io size in bytes or 0.
    pub fn get_topology_minimum_io_size(tp: blkid_topology) -> u64 {
        unsafe { blkid_topology_get_minimum_io_size(tp) }
    }

    /// Optimal io size in bytes or 0.
    pub fn get_topology_optimal_io_size(tp: blkid_topology) -> u64 {
        unsafe { blkid_topology_get_optimal_io_size(tp) }
    }

    /// Logical sector size (BLKSSZGET ioctl) in bytes or 0.
    pub fn get_topology_logical_sector_size(tp: blkid_topology) -> u64 {
        unsafe { blkid_topology_get_logical_sector_size(tp) }
    }

    /// Logical sector size (BLKSSZGET ioctl) in bytes or 0.
    pub fn get_topology_physical_sector_size(tp: blkid_topology) -> u64 {
        unsafe { blkid_topology_get_physical_sector_size(tp) }
    }

    /// Enables the partitions probing for non-binary interface.
    pub fn enable_partitions(&self) -> Result<&Self, BlkidError> {
        let ret_code = unsafe { blkid_probe_enable_partitions(self.probe, 1) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(self)
    }

    /// Sets probing flags to the partitions prober. This method is optional.
    /// BLKID_PARTS_* flags
    pub fn set_partition_flags(&self, flags: u32) -> Result<&Self, BlkidError> {
        let ret_code = unsafe { blkid_probe_set_partitions_flags(self.probe, flags as i32) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(self)
    }

    /// Enables the superblocks probing for non-binary interface.
    pub fn enable_superblocks(&self) -> Result<&Self, BlkidError> {
        let ret_code = unsafe { blkid_probe_enable_superblocks(self.probe, 1) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(self)
    }

    /// Sets probing flags to the superblocks prober. This method is optional, the default
    /// are BLKID_SUBLKS_DEFAULTS flags.
    /// flags are BLKID_SUBLKS_* flags
    pub fn set_superblock_flags(&self, flags: u32) -> Result<&Self, BlkidError> {
        let ret_code = unsafe { blkid_probe_set_superblocks_flags(self.probe, flags as i32) };
        if ret_code < 0 {
            return Err(BlkidError::get_error());
        }

        Ok(self)
    }

    // pub fn blkid_probe_get_partitions(pr: blkid_probe) -> blkid_partlist;
    // pub fn blkid_partlist_numof_partitions(ls: blkid_partlist)
    // -> ::std::os::raw::c_int;
    // pub fn blkid_partlist_get_table(ls: blkid_partlist) -> blkid_parttable;
    // pub fn blkid_partlist_get_partition(ls: blkid_partlist,
    // n: ::std::os::raw::c_int)
    // -> blkid_partition;
    // pub fn blkid_partlist_devno_to_partition(ls: blkid_partlist, devno: dev_t)
    // -> blkid_partition;
    // pub fn blkid_partition_get_table(par: blkid_partition) -> blkid_parttable;
    // pub fn blkid_partition_get_name(par: blkid_partition)
    // pub fn blkid_partition_get_uuid(par: blkid_partition) -> *const ::std::os::raw::c_char;
    // pub fn blkid_partition_get_partno(par: blkid_partition) -> ::std::os::raw::c_int;
    // pub fn blkid_partition_get_start(par: blkid_partition) -> blkid_loff_t;
    // pub fn blkid_partition_get_size(par: blkid_partition) -> blkid_loff_t;
    // pub fn blkid_partition_get_type(par: blkid_partition) -> ::std::os::raw::c_int;
    // pub fn blkid_partition_get_type_string(par: blkid_partition)
    // -> *const ::std::os::raw::c_char;
    // pub fn blkid_partition_get_flags(par: blkid_partition) -> ::std::os::raw::c_ulonglong;
    // pub fn blkid_partition_is_logical(par: blkid_partition) -> ::std::os::raw::c_int;
    // pub fn blkid_partition_is_extended(par: blkid_partition) -> ::std::os::raw::c_int;
    // pub fn blkid_partition_is_primary(par: blkid_partition) -> ::std::os::raw::c_int;
    // pub fn blkid_parttable_get_type(tab: blkid_parttable) -> *const ::std::os::raw::c_char;
    // pub fn blkid_parttable_get_offset(tab: blkid_parttable) -> blkid_loff_t;
    // pub fn blkid_parttable_get_parent(tab: blkid_parttable) -> blkid_partition;
}

impl Drop for BlkId {
    fn drop(&mut self) {
        if !self.probe.is_null() {
            unsafe {
                blkid_free_probe(self.probe);
            }
        }
    }
}

// pub fn blkid_put_cache(cache: blkid_cache);
// pub fn blkid_dev_set_search(iter: blkid_dev_iterate,
// search_type: *mut ::std::os::raw::c_char,
// search_value: *mut ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_devno_to_devname(devno: dev_t)
// -> *mut ::std::os::raw::c_char;
// pub fn blkid_devno_to_wholedisk(dev: dev_t,
// diskname: *mut ::std::os::raw::c_char,
// len: usize, diskdevno: *mut dev_t)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_all(cache: blkid_cache) -> ::std::os::raw::c_int;
// pub fn blkid_probe_all_new(cache: blkid_cache) -> ::std::os::raw::c_int;
// pub fn blkid_probe_all_removable(cache: blkid_cache)
// -> ::std::os::raw::c_int;
// pub fn blkid_get_dev(cache: blkid_cache,
// devname: *const ::std::os::raw::c_char,
// flags: ::std::os::raw::c_int) -> blkid_dev;
// pub fn blkid_get_dev_size(fd: ::std::os::raw::c_int) -> blkid_loff_t;
// pub fn blkid_get_tag_value(cache: blkid_cache,
// tagname: *const ::std::os::raw::c_char,
// devname: *const ::std::os::raw::c_char)
// -> *mut ::std::os::raw::c_char;
// pub fn blkid_get_devname(cache: blkid_cache,
// token: *const ::std::os::raw::c_char,
// value: *const ::std::os::raw::c_char)
// -> *mut ::std::os::raw::c_char;
// pub fn blkid_dev_has_tag(dev: blkid_dev,
// type_: *const ::std::os::raw::c_char,
// value: *const ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_find_dev_with_tag(cache: blkid_cache,
// type_: *const ::std::os::raw::c_char,
// value: *const ::std::os::raw::c_char)
// -> blkid_dev;
// pub fn blkid_parse_tag_string(token: *const ::std::os::raw::c_char,
// ret_type: *mut *mut ::std::os::raw::c_char,
// ret_val: *mut *mut ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_parse_version_string(ver_string:
// const ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_get_library_version(ver_string:
// mut *const ::std::os::raw::c_char,
// date_string:
// mut *const ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_encode_string(str: *const ::std::os::raw::c_char,
// str_enc: *mut ::std::os::raw::c_char,
// len: usize) -> ::std::os::raw::c_int;
// pub fn blkid_safe_string(str: *const ::std::os::raw::c_char,
// str_safe: *mut ::std::os::raw::c_char,
// len: usize) -> ::std::os::raw::c_int;
// pub fn blkid_send_uevent(devname: *const ::std::os::raw::c_char,
// action: *const ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_evaluate_tag(token: *const ::std::os::raw::c_char,
// value: *const ::std::os::raw::c_char,
// cache: *mut blkid_cache)
// -> *mut ::std::os::raw::c_char;
// pub fn blkid_evaluate_spec(spec: *const ::std::os::raw::c_char,
// cache: *mut blkid_cache)
// -> *mut ::std::os::raw::c_char;
// pub fn blkid_new_probe() -> blkid_probe;
// pub fn blkid_reset_probe(pr: blkid_probe);
// pub fn blkid_probe_set_device(pr: blkid_probe, fd: ::std::os::raw::c_int,
// off: blkid_loff_t, size: blkid_loff_t)
// -> ::std::os::raw::c_int;
// pub fn blkid_superblocks_get_name(idx: usize,
// name:
// mut *const ::std::os::raw::c_char,
// usage: *mut ::std::os::raw::c_int)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_reset_superblocks_filter(pr: blkid_probe)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_invert_superblocks_filter(pr: blkid_probe)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_filter_superblocks_type(pr: blkid_probe,
// flag: ::std::os::raw::c_int,
// names:
// mut *mut ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_filter_superblocks_usage(pr: blkid_probe,
// flag: ::std::os::raw::c_int,
// usage: ::std::os::raw::c_int)
// -> ::std::os::raw::c_int;
// pub fn blkid_known_pttype(pttype: *const ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_reset_partitions_filter(pr: blkid_probe)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_invert_partitions_filter(pr: blkid_probe)
// -> ::std::os::raw::c_int;
// pub fn blkid_probe_filter_partitions_type(pr: blkid_probe,
// flag: ::std::os::raw::c_int,
// names:
// mut *mut ::std::os::raw::c_char)
// -> ::std::os::raw::c_int;
