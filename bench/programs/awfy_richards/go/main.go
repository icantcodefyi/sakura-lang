// Richards from Are We Fast Yet (benchmarks/JavaScript/richards.js), "a derivation from the SOM
// version, which is derived from Mario Wolczko's Smalltalk version"; see ../LICENSE.
// AWFY's objects are structs linked by pointers (nil is NO_TASK / NO_WORK) and mutated in place.
// Deviation: AWFY's task functions are closures made with their data record; here the data
// record is a tagged struct and runTask switches on its kind, as every port of this program
// does. TRACING is false in AWFY; its trace code is left out. run(n) runs benchmark() n times
// and returns the number of true results.
package main

import ports "sakura-bench-ports"

const (
	idler = iota
	worker
	handlerA
	handlerB
	deviceA
	deviceB
	numTypes
)

const (
	devicePacketKind = 0
	workPacketKind   = 1
)

const dataSize = 4

type packet struct {
	link     *packet
	identity int
	kind     int
	datum    int
	data     [dataSize]int
}

// append is RBObject's append.
func appendPacket(p *packet, queueHead *packet) *packet {
	p.link = nil
	if queueHead == nil {
		return p
	}

	mouse := queueHead
	for mouse.link != nil {
		mouse = mouse.link
	}
	mouse.link = p
	return queueHead
}

type taskState struct {
	packetPending, taskWaiting, taskHolding bool
}

func createRunning() taskState           { return taskState{false, false, false} }
func createWaiting() taskState           { return taskState{false, true, false} }
func createWaitingWithPacket() taskState { return taskState{true, true, false} }

func (t *taskState) isTaskHoldingOrWaiting() bool {
	return t.taskHolding || (!t.packetPending && t.taskWaiting)
}

func (t *taskState) isWaitingWithPacket() bool {
	return t.packetPending && t.taskWaiting && !t.taskHolding
}

// The task data records; kind selects the task function.
type dataKind int

const (
	deviceData dataKind = iota
	handlerData
	idleData
	workerData
)

type dataRecord struct {
	kind dataKind
	// DeviceTaskDataRecord
	pending *packet
	// HandlerTaskDataRecord
	workIn, deviceIn *packet
	// IdleTaskDataRecord
	control, idleCount int
	// WorkerTaskDataRecord
	destination, count int
}

type taskControlBlock struct {
	taskState
	link     *taskControlBlock
	identity int
	priority int
	input    *packet
	handle   *dataRecord
}

func (t *taskControlBlock) addInputAndCheckPriority(p *packet, oldTask *taskControlBlock) *taskControlBlock {
	if t.input == nil {
		t.input = p
		t.packetPending = true
		if t.priority > oldTask.priority {
			return t
		}
	} else {
		t.input = appendPacket(p, t.input)
	}
	return oldTask
}

type scheduler struct {
	queuePacketCount    int
	holdCount           int
	taskTable           [numTypes]*taskControlBlock
	taskList            *taskControlBlock
	currentTask         *taskControlBlock
	currentTaskIdentity int
}

func (s *scheduler) runTask(t *taskControlBlock) *taskControlBlock {
	var message *packet
	if t.isWaitingWithPacket() {
		message = t.input
		t.input = message.link
		if t.input == nil {
			t.taskState = createRunning() // running()
		} else {
			t.taskState = taskState{true, false, false} // packetPending()
		}
	} else {
		message = nil
	}
	d := t.handle
	switch d.kind {
	case deviceData:
		return s.deviceFn(message, d)
	case handlerData:
		return s.handlerFn(message, d)
	case idleData:
		return s.idlerFn(d)
	default:
		return s.workerFn(message, d)
	}
}

// The task functions: AWFY's closures in createDevice, createHandler, createIdler and
// createWorker.

func (s *scheduler) deviceFn(workArg *packet, d *dataRecord) *taskControlBlock {
	functionWork := workArg
	if functionWork == nil {
		if functionWork = d.pending; functionWork == nil {
			return s.markWaiting()
		}
		d.pending = nil
		return s.queuePacket(functionWork)
	}
	d.pending = functionWork
	return s.holdSelf()
}

func (s *scheduler) handlerFn(work *packet, d *dataRecord) *taskControlBlock {
	if work != nil {
		if work.kind == workPacketKind {
			d.workIn = appendPacket(work, d.workIn)
		} else {
			d.deviceIn = appendPacket(work, d.deviceIn)
		}
	}

	workPacket := d.workIn
	if workPacket == nil {
		return s.markWaiting()
	}
	count := workPacket.datum
	if count >= dataSize {
		d.workIn = workPacket.link
		return s.queuePacket(workPacket)
	}
	devicePacket := d.deviceIn
	if devicePacket == nil {
		return s.markWaiting()
	}
	d.deviceIn = devicePacket.link
	devicePacket.datum = workPacket.data[count]
	workPacket.datum = count + 1
	return s.queuePacket(devicePacket)
}

func (s *scheduler) idlerFn(d *dataRecord) *taskControlBlock {
	d.idleCount--
	if d.idleCount == 0 {
		return s.holdSelf()
	}
	if d.control&1 == 0 {
		d.control /= 2
		return s.release(deviceA)
	}
	d.control = (d.control / 2) ^ 53256
	return s.release(deviceB)
}

func (s *scheduler) workerFn(work *packet, d *dataRecord) *taskControlBlock {
	if work == nil {
		return s.markWaiting()
	}

	if d.destination == handlerA {
		d.destination = handlerB
	} else {
		d.destination = handlerA
	}
	work.identity = d.destination
	work.datum = 0
	for i := 0; i < dataSize; i++ {
		d.count++
		if d.count > 26 {
			d.count = 1
		}
		work.data[i] = 65 + d.count - 1
	}
	return s.queuePacket(work)
}

func (s *scheduler) createPacket(link *packet, identity, kind int) *packet {
	return &packet{link: link, identity: identity, kind: kind}
}

func (s *scheduler) createTask(identity, priority int, work *packet, state taskState, data *dataRecord) {
	t := &taskControlBlock{
		taskState: state,
		link:      s.taskList,
		identity:  identity,
		priority:  priority,
		input:     work,
		handle:    data,
	}
	s.taskList = t
	s.taskTable[identity] = t
}

func (s *scheduler) findTask(identity int) *taskControlBlock {
	t := s.taskTable[identity]
	if t == nil {
		panic("findTask failed")
	}
	return t
}

func (s *scheduler) holdSelf() *taskControlBlock {
	s.holdCount++
	s.currentTask.taskHolding = true
	return s.currentTask.link
}

func (s *scheduler) queuePacket(p *packet) *taskControlBlock {
	t := s.findTask(p.identity)
	if t == nil {
		return nil
	}

	s.queuePacketCount++

	p.link = nil
	p.identity = s.currentTaskIdentity
	return t.addInputAndCheckPriority(p, s.currentTask)
}

func (s *scheduler) release(identity int) *taskControlBlock {
	t := s.findTask(identity)
	if t == nil {
		return nil
	}
	t.taskHolding = false
	if t.priority > s.currentTask.priority {
		return t
	}
	return s.currentTask
}

func (s *scheduler) markWaiting() *taskControlBlock {
	s.currentTask.taskWaiting = true
	return s.currentTask
}

func (s *scheduler) schedule() {
	s.currentTask = s.taskList
	for s.currentTask != nil {
		if s.currentTask.isTaskHoldingOrWaiting() {
			s.currentTask = s.currentTask.link
		} else {
			s.currentTaskIdentity = s.currentTask.identity
			s.currentTask = s.runTask(s.currentTask)
		}
	}
}

func (s *scheduler) start() bool {
	s.createTask(idler, 0, nil, createRunning(), &dataRecord{kind: idleData, control: 1, idleCount: 10000})
	workQ := s.createPacket(nil, worker, workPacketKind)
	workQ = s.createPacket(workQ, worker, workPacketKind)

	s.createTask(worker, 1000, workQ, createWaitingWithPacket(), &dataRecord{kind: workerData, destination: handlerA})
	workQ = s.createPacket(nil, deviceA, devicePacketKind)
	workQ = s.createPacket(workQ, deviceA, devicePacketKind)
	workQ = s.createPacket(workQ, deviceA, devicePacketKind)

	s.createTask(handlerA, 2000, workQ, createWaitingWithPacket(), &dataRecord{kind: handlerData})
	workQ = s.createPacket(nil, deviceB, devicePacketKind)
	workQ = s.createPacket(workQ, deviceB, devicePacketKind)
	workQ = s.createPacket(workQ, deviceB, devicePacketKind)

	s.createTask(handlerB, 3000, workQ, createWaitingWithPacket(), &dataRecord{kind: handlerData})
	s.createTask(deviceA, 4000, nil, createWaiting(), &dataRecord{kind: deviceData})
	s.createTask(deviceB, 5000, nil, createWaiting(), &dataRecord{kind: deviceData})

	s.schedule()

	return s.queuePacketCount == 23246 && s.holdCount == 9297
}

func run(innerIterations int64) int64 {
	sum := int64(0)
	for i := int64(0); i < innerIterations; i++ {
		if (&scheduler{}).start() {
			sum++
		}
	}
	return sum
}

func main() { ports.Main(run, 100000) }
