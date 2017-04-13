// #![feature(core)]
// #![feature(test)]
// #![feature(alloc)]
// #![feature(str_words)]

extern crate mmap;
// extern crate alloc;
// extern crate core;
// extern crate test;
extern crate byteorder;

extern crate docopt;
use docopt::Docopt;

use std::cmp::Ordering;

use std::fs::File;

use graph_iterator::{EdgeMapper, DeltaCompressedReaderMapper, NodesEdgesMemMapper, UpperLowerMemMapper};
use hilbert_curve::{encode, Decoder, convert_to_hilbert, BytewiseHilbert, to_hilbert, merge};
use twitter_parser::{ ReaderMapper, _parse_to_vertex };
use std::io::{BufReader, BufWriter, stdin, stdout};
use byteorder::{WriteBytesExt, LittleEndian};

mod typedrw;
mod hilbert_curve;
mod graph_iterator;
mod twitter_parser;

static USAGE: &'static str = "
Usage: COST pagerank  (vertex | hilbert | compressed) <prefix>
       COST label_prop (vertex | hilbert | compressed) <prefix>
       COST union_find (vertex | hilbert | compressed) <prefix>
       COST stats  (vertex | hilbert | compressed) <prefix>
       COST print  (vertex | hilbert | compressed) <prefix>
       COST to_hilbert [--dense] <prefix>
       COST parse_to_hilbert
       COST merge <source>...
       COST twitter <from> <prefix>
";


fn main() {
    let args = Docopt::new(USAGE).and_then(|dopt| dopt.parse()).unwrap_or_else(|e| e.exit());

    if args.get_bool("vertex") {
        let graph = NodesEdgesMemMapper::new(args.get_str("<prefix>"));
        // let graph = NodesEdgesMapper {
        //     nodes: || File::open(format!("{}.nodes", args.get_str("<prefix>"))).unwrap(),
        //     edges: || File::open(format!("{}.edges", args.get_str("<prefix>"))).unwrap(),
        // };
        if args.get_bool("stats") { stats(&graph); }
        if args.get_bool("print") { print(&graph); }
        if args.get_bool("pagerank") { pagerank(&graph, stats(&graph), 0.85f32); }
        if args.get_bool("label_prop") { label_propagation(&graph, stats(&graph)); }
        if args.get_bool("union_find") { union_find(&graph, stats(&graph)); }
    }

    if args.get_bool("hilbert") {
        let graph = UpperLowerMemMapper::new(args.get_str("<prefix>"));
        // let graph = UpperLowerMapper {
        //     upper: || File::open(format!("{}.nodes", args.get_str("<prefix>"))).unwrap(),
        //     lower: || File::open(format!("{}.edges", args.get_str("<prefix>"))).unwrap(),
        // };
        if args.get_bool("stats") { stats(&graph); }
        if args.get_bool("print") { print(&graph); }
        if args.get_bool("pagerank") { pagerank(&graph, stats(&graph), 0.85f32); }
        if args.get_bool("label_prop") { label_propagation(&graph, stats(&graph)); }
        if args.get_bool("union_find") { union_find(&graph, stats(&graph)); }
    }

    if args.get_bool("compressed") {
        let graph = DeltaCompressedReaderMapper::new(|| BufReader::new(File::open(args.get_str("<prefix>")).unwrap()));
        if args.get_bool("stats") { stats(&graph); }
        if args.get_bool("print") { print(&graph); }
        if args.get_bool("pagerank") { pagerank(&graph, stats(&graph), 0.85f32); }
        if args.get_bool("label_prop") { label_propagation(&graph, stats(&graph)); }
        if args.get_bool("union_find") { union_find(&graph, stats(&graph)); }
    }

    if args.get_bool("to_hilbert") {
        let prefix = args.get_str("<prefix>");
        let graph = NodesEdgesMemMapper::new(args.get_str("<prefix>"));
        // let graph = NodesEdgesMapper {
        //     nodes: || File::open(format!("{}.nodes", prefix)).unwrap(),
        //     edges: || File::open(format!("{}.edges", prefix)).unwrap(),
        // };

        let mut u_writer = BufWriter::new(File::create(format!("{}.upper", prefix)).unwrap());
        let mut l_writer = BufWriter::new(File::create(format!("{}.lower", prefix)).unwrap());

        convert_to_hilbert(&graph, args.get_bool("--dense"), |ux, uy, c, ls| {
            u_writer.write_u16::<LittleEndian>(ux).unwrap();
            u_writer.write_u16::<LittleEndian>(uy).unwrap();
            u_writer.write_u32::<LittleEndian>(c).unwrap();
            for &(lx, ly) in ls.iter(){
                l_writer.write_u16::<LittleEndian>(lx).unwrap();
                l_writer.write_u16::<LittleEndian>(ly).unwrap();
            }
        });
    }

    if args.get_bool("parse_to_hilbert") {
        let reader_mapper = ReaderMapper { reader: || BufReader::new(stdin())};
        let mut writer = BufWriter::new(stdout());

        let mut prev = 0u64;
        to_hilbert(&reader_mapper, |next| {
            assert!(prev < next);
            encode(&mut writer, next - prev);
            prev = next;
        });
    }

    if args.get_bool("merge") {
        let mut writer = BufWriter::new(stdout());
        let mut vector = Vec::new();
        for &source in args.get_vec("<source>").iter() {
            vector.push(Decoder::new(BufReader::new(File::open(source).unwrap())));
        }

        let mut prev = 0u64;
        merge(vector, |next| {
            assert!(prev < next);
            encode(&mut writer, next - prev);
            prev = next;
        });
    }

	if args.get_bool("twitter") {
		_parse_to_vertex(args.get_str("<from>"),args.get_str("<prefix>"))
	}
}

fn stats<G: EdgeMapper>(graph: &G) -> u32{
    let mut max_x = 0;
    let mut max_y = 0;
    let mut edges = 0;
    graph.map_edges(|x, y| {
        if max_x < x { max_x = x; }
        if max_y < y { max_y = y; }
        edges += 1;
    });

    println!("max x: {}", max_x);
    println!("max y: {}", max_y);
    println!("edges: {}", edges);
	std::cmp::max(max_x, max_y) + 1
}

fn print<G: EdgeMapper>(graph: &G) {
    let hilbert = BytewiseHilbert::new();
    graph.map_edges(|x, y| { println!("{}\t{} -> {}", x, y, hilbert.entangle((x,y))) });
}

fn pagerank<G: EdgeMapper>(graph: &G, nodes: u32, alpha: f32)
{
    let mut src: Vec<f32> = (0..nodes).map(|_| 0f32).collect();
    let mut dst: Vec<f32> = (0..nodes).map(|_| 0f32).collect();
    let mut deg: Vec<f32> = (0..nodes).map(|_| 0f32).collect();

    graph.map_edges(|x, _| { deg[x as usize] += 1f32 });

    for _iteration in 0 .. 20 {
        println!("Iteration: {}", _iteration);
        for node in 0 .. nodes {
            src[node as usize] = alpha * dst[node as usize] / deg[node as usize];
            dst[node as usize] = 1f32 - alpha;
        }

        graph.map_edges(|x, y| { dst[y as usize] += src[x as usize]; });

        // UNSAFE: graph.map_edges(|x, y| { unsafe { *dst.as_mut_slice().get_unchecked_mut(y) += *src.as_mut_slice().get_unchecked_mut(x); }});
    }
}

fn union_find<G: EdgeMapper>(graph: &G, nodes: u32)
{
    let mut roots: Vec<u32> = (0..nodes).collect();   // u32 works, and is smaller than uint/u64
    let mut ranks: Vec<u8> = (0..nodes).map(|_| 0u8).collect();          // u8 should be large enough (n < 2^256)

    graph.map_edges(|mut x, mut y| {

        x = roots[x as usize];
        y = roots[y as usize];

        // x = unsafe { *roots.as_mut_slice().get_unchecked_mut(x as usize) };
        // y = unsafe { *roots.as_mut_slice().get_unchecked_mut(y as usize) };

        while x != roots[x as usize] { x = roots[x as usize]; }
        while y != roots[y as usize] { y = roots[y as usize]; }

        // unsafe { while x != *roots.as_mut_slice().get_unchecked_mut(x as usize) { x = *roots.as_mut_slice().get_unchecked_mut(x as usize); } }
        // unsafe { while y != *roots.as_mut_slice().get_unchecked_mut(y as usize) { y = *roots.as_mut_slice().get_unchecked_mut(y as usize); } }

        if x != y {
            match ranks[x as usize].cmp(&ranks[y as usize]) {
                Ordering::Less    => roots[x as usize] = y as u32,
                Ordering::Greater => roots[y as usize] = x as u32,
                Ordering::Equal   => { roots[y as usize] = x as u32; ranks[x as usize] += 1 },
            }
        }

        // works for Hilbert curve order
        // roots[x as usize] = min(x, y);
        // roots[y as usize] = min(x, y);
    });

    let mut non_roots = 0u32;
    for i in 0 .. roots.len() { if i as u32 != roots[i] { non_roots += 1; }}
    println!("{} non-roots found", non_roots);
}

fn label_propagation<G: EdgeMapper>(graph: &G, nodes: u32)
{
    let mut label: Vec<u32> = (0..nodes).collect();
    let mut old_sum: u64 = label.iter().fold(0, |t,x| t + *x as u64) + 1;
    let mut new_sum: u64 = label.iter().fold(0, |t,x| t + *x as u64);

    while new_sum < old_sum {
        graph.map_edges(|src, dst| {
            match label[src as usize].cmp(&label[dst as usize]) {
                Ordering::Less    => label[dst as usize] = label[src as usize],
                Ordering::Greater => label[src as usize] = label[dst as usize],
                Ordering::Equal   => { },
            }
        });

        old_sum = new_sum;
        new_sum = label.iter().fold(0, |t,x| t + *x as u64);
        println!("iteration");
    }

    let mut non_roots = 0u32;
    for i in 0 .. label.len() { if i as u32 != label[i] { non_roots += 1; }}
    println!("{} non-roots found", non_roots);
}
