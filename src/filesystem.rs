#![allow(clippy::unnecessary_mut_passed)]
#![deny(clippy::unimplemented)]

use crate::config;
use crate::client;

use polyfuse::{
    io::{Reader, Writer},
    op,
    reply::{Reply, ReplyAttr, ReplyEntry, ReplyOpen},
    Context, DirEntry, FileAttr, Filesystem, Operation,
};
use slab::Slab;

use crate::client::HTTP;
use std::path::PathBuf;
use std::{
    collections::hash_map::{Entry, HashMap},
    ffi::{OsStr, OsString},
    fmt::Debug,
    io,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use tracing_futures::Instrument;
use std::io::{Error, ErrorKind};
type Ino = u64;

//noinspection RsUnresolvedReference
#[derive(Debug)]
struct INodeTable {
    map: HashMap<Ino, Arc<Mutex<INode>>>,
    next_ino: Ino,
}

impl INodeTable {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_ino: 1, // a
        }
    }

    fn vacant_entry(&mut self) -> VacantEntry<'_> {
        let ino = self.next_ino;
        VacantEntry { table: self, ino }
    }

    //noinspection RsUnresolvedReference
    fn get(&self, ino: Ino) -> Option<Arc<Mutex<INode>>> {
        self.map.get(&ino).cloned()
    }
}

#[derive(Debug)]
struct VacantEntry<'a> {
    table: &'a mut INodeTable,
    ino: u64,
}

impl VacantEntry<'_> {
    fn ino(&self) -> Ino {
        self.ino
    }

    //noinspection RsUnresolvedReference
    fn insert(self, inode: INode) {
        let Self { table, ino } = self;
        table.map.insert(ino, Arc::new(Mutex::new(inode)));
        table.next_ino += 1;
    }
}

#[derive(Debug)]
struct INode {
    attr: FileAttr,
    xattrs: HashMap<OsString, Arc<Vec<u8>>>,
    refcount: u64,
    links: u64,
    kind: INodeKind,
}

#[derive(Debug)]
enum INodeKind {
    RegularFile(Vec<u8>),
    Directory(Directory),
}

#[derive(Debug, Clone)]
struct Directory {
    children: HashMap<OsString, Ino>,
    parent: Option<Ino>,
}

impl Directory {
    fn collect_entries(&self, attr: &FileAttr) -> Vec<Arc<DirEntry>> {
        let mut entries = Vec::with_capacity(self.children.len() + 2);
        let mut offset: u64 = 1;

        entries.push(Arc::new(DirEntry::dir(".", attr.ino(), offset)));
        offset += 1;

        entries.push(Arc::new(DirEntry::dir(
            "..",
            self.parent.unwrap_or_else(|| attr.ino()),
            offset,
        )));
        offset += 1;

        for (name, &ino) in &self.children {
            entries.push(Arc::new(DirEntry::new(name, ino, offset)));
            offset += 1;
        }

        entries
    }
}

#[derive(Debug)]
struct DirHandle {
    entries: Vec<Arc<DirEntry>>,
}

#[derive(Debug)]
struct FileInodeMap {
    parent: Ino,
    ino: Ino,
    path: PathBuf,
}

//noinspection RsUnresolvedReference
//noinspection RsUnresolvedReference
//noinspection RsUnresolvedReference
#[derive(Debug)]
pub struct MemFS {
    http: client::HTTP,
    inodes: Mutex<INodeTable>,
    f_ino_map: Mutex<Vec<FileInodeMap>>,
    ttl: Duration,
    dir_handles: Mutex<Slab<Arc<Mutex<DirHandle>>>>,
    cfg: config::Config,
}

impl MemFS {
    //noinspection RsUnresolvedReference
    //noinspection RsUnresolvedReference
    pub fn new(cfg: &config::Config) -> Self {
        let mut inodes = INodeTable::new();
        //let self.cfg = cfg;
        //let entries = http::list_directory(&cfg.server, &cfg.username, &cfg.password, "/").await;
        inodes.vacant_entry().insert(INode {
            attr: {
                let mut attr = FileAttr::default();
                attr.set_ino(1);
                attr.set_nlink(2);
                attr.set_mode(libc::S_IFDIR | 0o755);
                attr
            },
            xattrs: HashMap::new(),
            refcount: u64::max_value() / 2,
            links: u64::max_value() / 2,
            kind: INodeKind::Directory(Directory {
                children: HashMap::new(),
                parent: None,
            }),
        });

        Self {
            http: HTTP::new(
                cfg.server.clone(),
                cfg.username.clone(),
                cfg.password.clone(),
            ),
            inodes: Mutex::new(inodes),
            f_ino_map: Mutex::new(Vec::new()),
            dir_handles: Mutex::default(),
            ttl: Duration::from_secs(60 * 60 * 24),
            cfg: cfg.clone(),
        }
    }

    fn make_entry_reply(&self, ino: Ino, attr: FileAttr) -> ReplyEntry {
        let mut reply = ReplyEntry::default();
        reply.ino(ino);
        reply.attr(attr);
        reply.ttl_entry(self.ttl);
        reply
    }

    async fn lookup_inode(&self, parent: Ino, name: &OsStr) -> io::Result<ReplyEntry> {
        debug!("==> lookup_inode: parent: {:?}, name: {:?}", parent, name);

        let inodes = self.inodes.lock().await;

        let parent = inodes.get(parent).ok_or_else(no_entry)?;
        let parent = parent.lock().await;

        let parent = match parent.kind {
            INodeKind::Directory(ref dir) => dir,
            _ => return Err(io::Error::from_raw_os_error(libc::ENOTDIR)),
        };

        let child_ino = parent.children.get(&*name).copied().ok_or_else(no_entry)?;
        let child = inodes.get(child_ino).unwrap_or_else(|| unreachable!());
        let mut child = child.lock().await;
        child.refcount += 1;
        Ok(self.make_entry_reply(child_ino, child.attr))
    }

    async fn make_node<F>(&self, parent: Ino, name: &OsStr, f: F) -> io::Result<ReplyEntry>
    where
        F: FnOnce(&VacantEntry<'_>) -> INode,
    {
        debug!("make_node: parent: {:?}, name: {:?}", parent, name);
        let mut inodes = self.inodes.lock().await;

        let parent = inodes.get(parent).ok_or_else(no_entry)?;
        let mut parent = parent.lock().await;

        let parent = match parent.kind {
            INodeKind::Directory(ref mut dir) => dir,
            _ => return Err(io::Error::from_raw_os_error(libc::ENOTDIR)),
        };
        match parent.children.entry(name.into()) {
            Entry::Occupied(..) => Err(io::Error::from_raw_os_error(libc::EEXIST)),
            Entry::Vacant(map_entry) => {
                let inode_entry = inodes.vacant_entry();
                let inode = f(&inode_entry);

                let reply = self.make_entry_reply(inode_entry.ino(), inode.attr);
                map_entry.insert(inode_entry.ino());
                inode_entry.insert(inode);

                Ok(reply)
            }
        }
    }

    async fn full_path(&self, parent_ino: u64) -> io::Result<PathBuf> {
        let mut full_uri: PathBuf = PathBuf::new();
        if parent_ino == 1 {
            full_uri = PathBuf::from("/");
        } else {
            let mut vec_full_uri: Vec<PathBuf> = Vec::new();
            let mut inode = parent_ino;
            loop {
                let p = self.inode_to_name(inode).await.unwrap();
                if p.1 != 0 {
                    vec_full_uri.push(p.0);
                    inode = p.1;
                } else {
                    break;
                }
            }
            vec_full_uri.push(PathBuf::from("/"));
            vec_full_uri.reverse();

            for dir in vec_full_uri {
                full_uri.push(dir);
            }
        }
        Ok(full_uri)
    }

    async fn name_to_inode(&self, p_inode: u64, name: &OsStr) -> Option<u64> {
        let inodes = self.inodes.lock().await;
        match inodes.get(p_inode).ok_or_else(no_entry) {
            Ok(inode) => {
                let inode = inode.lock().await;
                debug!("name_to_inode: p_inode - '{:?}' name - '{:?}'", p_inode, name);

                match &inode.kind {
                    INodeKind::Directory(ref dir) => match dir.children.get(name) {
                        Some(name) => Some(name.clone()),
                        None => None,
                    },
                    _ => None,
                }
            }
            Err(_e) => None,
        }
    }

    async fn do_lookup(&self, op: &op::Lookup<'_>) -> io::Result<ReplyEntry> {
        debug!("do_lookup: {:?}", op);
        match self.name_to_inode(op.parent(), op.name()).await {
            Some(f_inode) => {
                let inodes = self.inodes.lock().await;
                let inode = inodes.get(f_inode).ok_or_else(no_entry)?;
                let inode = inode.lock().await;
                match &inode.kind {
                    INodeKind::Directory(_) => {
                        drop(inode);
                        drop(inodes);
                        let mut file_path = self.full_path(op.parent()).await.unwrap();
                        file_path.push(op.name());
                        // self.fetch_remote(file_path, f_inode).await.unwrap();
                        match self.fetch_remote(file_path, f_inode).await {
                            Err(_) => return Err(io::Error::from_raw_os_error(libc::ENODATA)),
                            _ => {}
                        }
                    }
                    _ => {
                        drop(inode);
                        drop(inodes);
                    }
                };
            }
            None => warn!("do_lookup: Cant find inode for {:?}", op.name()),
        }

        self.lookup_inode(op.parent(), op.name()).await
    }

    async fn do_getattr(&self, op: &op::Getattr<'_>) -> io::Result<ReplyAttr> {
        // debug!("do_getattr: op: {:?}", op);
        let inodes = self.inodes.lock().await;

        let inode = inodes.get(op.ino()).ok_or_else(no_entry)?;
        let inode = inode.lock().await;

        let mut reply = ReplyAttr::new(inode.attr);
        reply.ttl_attr(self.ttl);

        Ok(reply)
    }

    pub async fn fetch_remote(&self, path: PathBuf, parent: u64) -> io::Result<()> {
        let remote_entries = match self.http.list(path).await {
            Ok(remote_entries) => remote_entries,
            Err (e) => return Err(Error::new(ErrorKind::Other, format!("HTTP {:?}: {}", e.status(), e)))
        };
        for r_entry in remote_entries.iter() {
            match &r_entry.r#type {
                Some(r#type) => match r#type.as_str() {
                    "file" => {
                        let f_name = r_entry.name.as_ref().unwrap();
                        let mut inode_map = self.f_ino_map.lock().await;
                        let mut full_name = self.full_path(parent).await.unwrap();
                        full_name.push(PathBuf::from(f_name));
                        let _x = self.make_node(parent, OsStr::new(f_name.as_str()), |entry| INode {
                            attr: {
                                debug!("fetch_remote: Adding file {:?}", full_name);
                                inode_map.push(FileInodeMap {
                                    parent,
                                    ino: entry.ino(),
                                    path: full_name,
                                });
                                let mut attr = FileAttr::default();
                                attr.set_ino(entry.ino());
                                attr.set_mtime(r_entry.parse_rfc2822());
                                attr.set_size(r_entry.size.unwrap());
                                attr.set_nlink(1);
                                attr.set_mode(libc::S_IFREG | 0o444);
                                attr
                            },
                            xattrs: HashMap::new(),
                            refcount: 1,
                            links: 1,
                            kind: INodeKind::RegularFile(vec![]),
                        })
                        .await;
                    }
                    "directory" => {
                        let f_name = r_entry.name.as_ref().unwrap();
                        let _x = self.make_node(parent, OsStr::new(f_name.as_str()), |entry| INode {
                            attr: {
                                debug!("fetch_remote: Adding directory {:?} - {:?}", f_name, parent);
                                let mut attr = FileAttr::default();
                                attr.set_ino(entry.ino());
                                attr.set_mtime(r_entry.parse_rfc2822());
                                attr.set_nlink(1);
                                attr.set_mode(libc::S_IFDIR | 0o755);
                                attr
                            },
                            xattrs: HashMap::new(),
                            refcount: u64::max_value() / 2,
                            links: u64::max_value() / 2,
                            kind: INodeKind::Directory(Directory {
                                children: HashMap::new(),
                                parent: Some(parent),
                            }),
                        })
                        .await;
                    }
                    &_ => {}
                },
                None => {}
            }
        }
        Ok(())
    }

    async fn inode_to_name(&self, inode: u64) -> Option<(PathBuf, u64)> {
        let inodes = self.inodes.lock().await;

        let inode_mutex = inodes.get(inode).ok_or_else(no_entry).unwrap();

        let inode_mutex = inode_mutex.lock().await;

        let mut parent_ino: u64 = 0;
        let mut uri = PathBuf::new();
        let ret = match &inode_mutex.kind {
            INodeKind::Directory(dir) => match dir.parent {
                Some(parent) => {
                    let par_inode = inodes.get(parent).ok_or_else(no_entry).unwrap();
                    let par_inode = par_inode.lock().await;

                    parent_ino = par_inode.attr.ino();

                    let _uri = match &par_inode.kind {
                        INodeKind::Directory(dir) => {
                            let _children = dir.children.clone();
                            for (name, c_inode) in &_children {
                                if &inode == c_inode {
                                    uri.push(name.as_os_str());
                                }
                            }
                            Some((uri, parent_ino))
                        }
                        _ => Some((uri, parent_ino)),
                    };
                    _uri
                }
                None => Some((uri, parent_ino)),
            },
            _ => Some((uri, parent_ino)),
        };
        ret
    }

    //noinspection RsUnresolvedReference
    async fn do_opendir(&self, op: &op::Opendir<'_>) -> io::Result<ReplyOpen> {
        debug!("do_opendir: {:?}", op);

        let mut dirs = self.dir_handles.lock().await;
        let inodes = self.inodes.lock().await;

        let inode = inodes.get(op.ino()).ok_or_else(no_entry)?;

        let inode = inode.lock().await;

        if inode.attr.nlink() == 0 {
            return Err(no_entry());
        }
        let dir = match inode.kind {
            INodeKind::Directory(ref dir) => dir,
            _ => return Err(io::Error::from_raw_os_error(libc::ENOTDIR)),
        };

        let key = dirs.insert(Arc::new(Mutex::new(DirHandle {
            entries: dir.collect_entries(&inode.attr),
        })));

        Ok(ReplyOpen::new(key as u64))
    }

    async fn do_readdir(&self, op: &op::Readdir<'_>) -> io::Result<impl Reply + Debug> {
        debug!("do_readdir: op: {:?}", op);
        let dirs = self.dir_handles.lock().await;

        let dir = dirs
            .get(op.fh() as usize)
            .cloned()
            .ok_or_else(unknown_error)?;
        let dir = dir.lock().await;

        let mut total_len = 0;
        let entries: Vec<_> = dir
            .entries
            .iter()
            .skip(op.offset() as usize)
            .take_while(|entry| {
                let entry: &DirEntry = &*entry;
                total_len += entry.as_ref().len() as u32;
                total_len < op.size()
            })
            .cloned()
            .collect();

        Ok(entries)
    }

    async fn do_releasedir(&self, op: &op::Releasedir<'_>) -> io::Result<()> {
        let mut dirs = self.dir_handles.lock().await;

        let dir = dirs.remove(op.fh() as usize);
        drop(dir);

        Ok(())
    }

    async fn do_read(&self, op: &op::Read<'_>) -> io::Result<impl Reply + Debug> {
        let full_path_mutex = self.f_ino_map.lock().await;
        let mut counter = 0;
        let full_path = loop {
            if counter < full_path_mutex.len() {
                if full_path_mutex[counter].ino == op.ino() {
                    break full_path_mutex[counter].path.clone();
                }
            } else {
                break PathBuf::from("");
            }
            counter += 1;
        };
        let offset = op.offset() as usize;
        let size = op.size() as usize;
        drop(full_path_mutex);
        let chunk = match self.http.read(full_path, size, offset).await {
            Ok(data) => data,
            Err (e) => {
                let msg =  format!("HTTP {:?}: {}", e.status(), e);
                error!("Read error. {:?}", msg);
                return Err(Error::new(ErrorKind::Other, msg))
            }
        };
        Ok(chunk.to_vec())
    }
}

#[polyfuse::async_trait]
impl Filesystem for MemFS {
    #[allow(clippy::cognitive_complexity)]
    async fn call<'a, 'cx, T: ?Sized>(
        &'a self,
        cx: &'a mut Context<'cx, T>,
        op: Operation<'cx>,
    ) -> io::Result<()>
    where
        T: Reader + Writer + Send + Unpin,
    {
        let span = tracing::debug_span!("MemFS::call", unique = cx.unique());
        span.in_scope(|| tracing::debug!(?op));

        macro_rules! try_reply {
            ($e:expr) => {
                match ($e).instrument(span.clone()).await {
                    Ok(reply) => {
                        span.in_scope(|| tracing::debug!(reply=?reply));
                        cx.reply(reply).await
                    }
                    Err(err) => {
                        let errno = err.raw_os_error().unwrap_or(libc::EIO);
                        span.in_scope(|| tracing::debug!(errno=errno));
                        cx.reply_err(errno).await
                    }
                }
            };
        }

        match op {
            Operation::Lookup(op) => try_reply!(self.do_lookup(&op)),
            Operation::Getattr(op) => try_reply!(self.do_getattr(&op)),
            Operation::Opendir(op) => try_reply!(self.do_opendir(&op)),
            Operation::Readdir(op) => try_reply!(self.do_readdir(&op)),
            Operation::Releasedir(op) => try_reply!(self.do_releasedir(&op)),
            Operation::Read(op) => try_reply!(self.do_read(&op)),
            _ => {
                span.in_scope(|| tracing::debug!("NOSYS"));
                Ok(())
            }
        }
    }
}

fn no_entry() -> io::Error {
    io::Error::from_raw_os_error(libc::ENOENT)
}

fn unknown_error() -> io::Error {
    io::Error::from_raw_os_error(libc::EIO)
}